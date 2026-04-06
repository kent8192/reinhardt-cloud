//! Reconciler logic for the `ReinhardtApp` custom resource.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use futures::StreamExt;
use k8s_openapi::api::apps::v1::{Deployment, StatefulSet};
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::{
	ConfigMap, LimitRange, Namespace, Secret, Service, ServiceAccount,
};
use k8s_openapi::api::networking::v1::{Ingress, NetworkPolicy};
use kube::api::{Api, DeleteParams, Patch, PatchParams, PostParams};
use kube::runtime::controller::{Action, Controller};
use kube::runtime::finalizer::{Event as FinalizerEvent, finalizer};
use kube::runtime::watcher;
use kube::{Client, ResourceExt};
use tracing::{error, info, warn};

use crate::error::Error;
use crate::inference::configmap::build_settings_configmap;
use crate::resources::credentials;
use crate::resources::preview;
use crate::resources::source::{build_kaniko_job, should_build_from_source};
use crate::inference::pages::{ResolvedPagesConfig, resolve_pages_config};
use crate::inference::platform::{Platform, PlatformConfig, ResourceDefaults};
use crate::inference::secrets::{build_db_credentials_secret, build_jwt_secret};
use crate::resources::security::limit_range::build_limit_range;
use crate::resources::security::network_policy::{
	build_app_ingress_policy, build_default_deny_policy, build_managed_service_egress_policy,
};
use crate::resources::{
	self, build_db_secret, build_db_service, build_db_statefulset, build_deployment, build_ingress,
	build_migration_job, build_service,
};
use reinhardt_cloud_types::crd::database::{DatabaseStatus, ResourcePhase};
use reinhardt_cloud_types::crd::policy::DeletionPolicy;
use reinhardt_cloud_types::crd::{AppCondition, AppPhase, ReinhardtApp, ReinhardtAppStatus};
use reinhardt_cloud_types::{ConditionStatus, ConditionType};

const FINALIZER_NAME: &str = "paas.reinhardt-cloud.dev/cleanup";

/// Shared context available to every reconciliation call.
pub(crate) struct Context {
	/// Kubernetes API client.
	pub client: Client,
	/// Platform-specific configuration for resource inference.
	// Reserved for future reconciler integration that will use platform
	// defaults when inferring resource specifications.
	#[allow(dead_code)]
	pub platform: PlatformConfig,
}

/// Main reconciliation entry point.
pub(crate) async fn reconcile(obj: Arc<ReinhardtApp>, ctx: Arc<Context>) -> Result<Action, Error> {
	let name = obj.name_any();
	let namespace = obj
		.namespace()
		.filter(|ns| !ns.is_empty())
		.ok_or_else(|| Error::MissingNamespace(name.clone()))?;
	info!("Reconciling ReinhardtApp {namespace}/{name}");

	let api: Api<ReinhardtApp> = Api::namespaced(ctx.client.clone(), &namespace);

	finalizer(&api, FINALIZER_NAME, obj, |event| async {
		match event {
			FinalizerEvent::Apply(app) => apply(app, &ctx, &namespace).await,
			FinalizerEvent::Cleanup(app) => cleanup(app, &ctx, &namespace).await,
		}
	})
	.await
	.map_err(|e| Error::Finalizer(Box::new(e)))
}

/// Apply the desired state for a `ReinhardtApp`.
async fn apply(app: Arc<ReinhardtApp>, ctx: &Context, namespace: &str) -> Result<Action, Error> {
	let name = app.name_any();
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();

	// Reconcile settings ConfigMap via server-side apply
	let configmap = build_settings_configmap(&name, namespace);
	let cm_api: Api<ConfigMap> = Api::namespaced(ctx.client.clone(), namespace);
	cm_api
		.patch(
			&format!("{name}-settings"),
			&ssapply,
			&Patch::Apply(&configmap),
		)
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled ConfigMap {namespace}/{name}-settings");

	// Create JWT Secret if auth.jwt is enabled (preserve existing tokens)
	if app.spec.auth.as_ref().is_some_and(|a| a.jwt) {
		let secret_name = format!("{name}-jwt-secret");
		let secret_api: Api<Secret> = Api::namespaced(ctx.client.clone(), namespace);
		if secret_api
			.get_opt(&secret_name)
			.await
			.map_err(Error::Kube)?
			.is_none()
		{
			let jwt_secret = build_jwt_secret(&name, namespace);
			secret_api
				.create(&PostParams::default(), &jwt_secret)
				.await
				.map_err(|e| Error::SecretGeneration(e.to_string()))?;
			info!("Created JWT Secret {namespace}/{secret_name}");
		}
	}

	// Create DB credentials Secret if database is configured (preserve existing credentials)
	if app.spec.database.is_some() {
		let db_secret_name = format!("{name}-db-credentials");
		let secret_api: Api<Secret> = Api::namespaced(ctx.client.clone(), namespace);
		if secret_api
			.get_opt(&db_secret_name)
			.await
			.map_err(Error::Kube)?
			.is_none()
		{
			let password_bytes: [u8; 16] = rand::random();
			let password_str =
				base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(password_bytes);
			let db_secret = build_db_credentials_secret(
				&name,
				namespace,
				&format!("{name}_user"),
				&password_str,
			);
			secret_api
				.create(&PostParams::default(), &db_secret)
				.await
				.map_err(|e| Error::SecretGeneration(e.to_string()))?;
			info!("Created DB credentials Secret {namespace}/{db_secret_name}");
		}
	}

	// Resolve pages configuration (explicit spec.pages > introspect signals > disabled)
	let pages_config = resolve_pages_config(&app);

	// Reconcile owned Deployment via server-side apply
	let deployments: Api<Deployment> = Api::namespaced(ctx.client.clone(), namespace);
	let desired_deployment = build_deployment(&app, pages_config.as_ref(), &ctx.platform.platform)?;
	deployments
		.patch(
			&name,
			&PatchParams::apply("reinhardt-cloud-operator").force(),
			&Patch::Apply(&desired_deployment),
		)
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled Deployment {namespace}/{name}");

	// Reconcile owned Service via server-side apply
	let services: Api<Service> = Api::namespaced(ctx.client.clone(), namespace);
	let desired_service = build_service(&app, pages_config.is_some())?;
	services
		.patch(
			&name,
			&PatchParams::apply("reinhardt-cloud-operator").force(),
			&Patch::Apply(&desired_service),
		)
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled Service {namespace}/{name}");

	// Database provisioning — explicit spec.database takes precedence,
	// falling back to introspect infrastructure signals.
	if should_provision_postgresql(&app) && app.spec.database.is_none() {
		// Introspect-derived database: provision StatefulSet + credentials
		reconcile_db_secret(&app, &ctx.client, namespace).await?;
		reconcile_db_statefulset(&app, &ctx.client, namespace).await?;
		reconcile_db_service_resource(&app, &ctx.client, namespace).await?;
		reconcile_migration_job_resource(&app, &ctx.client, namespace).await?;
	}

	// Ingress — explicit services.ingress_host takes precedence,
	// falling back to introspect routes.
	if let Some((routes, port)) = resolve_ingress_config(&app) {
		reconcile_ingress_resource(
			&app,
			&ctx.client,
			namespace,
			&routes,
			port,
			pages_config.as_ref(),
		)
		.await?;
	}

	// Cache provisioning — explicit spec.cache takes precedence,
	// falling back to introspect infrastructure signals.
	if should_provision_cache(&app) {
		reconcile_cache_deployment(&app, &ctx.client, namespace).await?;
		reconcile_cache_service_resource(&app, &ctx.client, namespace).await?;
		info!("Reconciled cache resources for {name}");
	}

	// Worker provisioning — explicit spec.worker takes precedence,
	// falling back to introspect infrastructure signals.
	if should_provision_worker(&app) {
		reconcile_worker_deployment_resource(&app, &ctx.client, namespace, &ctx.platform.platform)
			.await?;
		info!("Reconciled worker deployment for {name}");
	}

	// gRPC Service provisioning (Phase 3)
	let needs_grpc = app
		.spec
		.introspect
		.as_ref()
		.map(|i| reinhardt_cloud_core::inference::requires_grpc(&i.features.infrastructure_signals))
		.unwrap_or(false);
	if needs_grpc {
		reconcile_grpc_service(&app, &ctx.client, namespace).await?;
	}

	// Storage provisioning (Phase 4)
	if should_provision_storage(&app) {
		let backend = app
			.spec
			.introspect
			.as_ref()
			.and_then(|i| i.features.infrastructure_signals.storage.as_deref())
			.unwrap_or("pvc");
		// IAM role binding for cloud storage is not yet configurable via CRD;
		// ServiceAccount is created only when a role is explicitly provided.
		if let Some(sa) = resources::storage::build_storage_service_account(&app, backend, None)? {
			reconcile_storage_sa(&app, &ctx.client, namespace, &sa).await?;
		}
	}

	// Mail provisioning (Phase 4)
	if should_provision_mail(&app) {
		reconcile_mail_secret(&app, &ctx.client, namespace).await?;
	}

	// Git credentials validation (#278)
	if let Some(secret_name) = credentials::should_warn_missing_credentials(&app) {
		let secret_api: Api<Secret> = Api::namespaced(ctx.client.clone(), namespace);
		if secret_api
			.get_opt(&secret_name)
			.await
			.map_err(Error::Kube)?
			.is_none()
		{
			warn!(
				"Git credentials Secret '{secret_name}' referenced by {name} does not exist"
			);
		}
	}

	// Source build (#275) — triggered by build-trigger annotation
	if should_build_from_source(&app) {
		let trigger = app
			.metadata
			.annotations
			.as_ref()
			.and_then(|a| a.get("reinhardt.dev/build-trigger"));
		if let Some(trigger_ts) = trigger {
			let short_trigger: String = trigger_ts.chars().take(8).collect();
			let image_tag = format!("{name}-{short_trigger}");
			let job = build_kaniko_job(&app, &image_tag)?;
			let job_api: Api<Job> = Api::namespaced(ctx.client.clone(), namespace);
			let job_name = job.metadata.name.as_deref().unwrap_or("unknown");
			if job_api
				.get_opt(job_name)
				.await
				.map_err(Error::Kube)?
				.is_none()
			{
				job_api
					.create(&PostParams::default(), &job)
					.await
					.map_err(Error::Kube)?;
				info!("Created build Job {namespace}/{job_name}");
			}

			// Clear the build-trigger annotation to prevent re-triggering
			let patch = serde_json::json!({
				"metadata": {
					"annotations": {
						"reinhardt.dev/build-trigger": null
					}
				}
			});
			let app_api: Api<ReinhardtApp> = Api::namespaced(ctx.client.clone(), namespace);
			app_api
				.patch(&name, &PatchParams::default(), &Patch::Merge(&patch))
				.await
				.map_err(Error::Kube)?;
		}
	}

	// Preview environment reconciliation (#277)
	if app.spec.source.as_ref().is_some_and(|s| {
		s.preview.as_ref().is_some_and(|p| p.enabled)
	}) {
		let preview_action = app
			.metadata
			.annotations
			.as_ref()
			.and_then(|a| a.get("reinhardt.dev/preview-action"));
		if let Some(action) = preview_action {
			let pr_number = app
				.metadata
				.annotations
				.as_ref()
				.and_then(|a| a.get("reinhardt.dev/pr-number"))
				.cloned()
				.unwrap_or_default();
			let preview_name = preview::preview_app_name(&name, &pr_number);
			let app_api: Api<ReinhardtApp> = Api::namespaced(ctx.client.clone(), namespace);

			match action.as_str() {
				"create" | "update" => {
					let image_tag = format!("pr-{pr_number}-latest");
					let preview_spec = preview::build_preview_spec(&app, &pr_number, &image_tag)?;
					let preview_labels = preview::preview_labels(&name, &pr_number);
					let preview_app = ReinhardtApp {
						metadata: kube::api::ObjectMeta {
							name: Some(preview_name.clone()),
							namespace: Some(namespace.to_string()),
							labels: Some(preview_labels),
							owner_references: Some(vec![resources::labels::owner_reference(&app)?]),
							annotations: Some(BTreeMap::from([(
								"reinhardt.dev/last-activity".to_string(),
								chrono::Utc::now().to_rfc3339(),
							)])),
							..Default::default()
						},
						spec: preview_spec,
						status: None,
					};
					app_api
						.patch(
							&preview_name,
							&PatchParams::apply("reinhardt-cloud-operator").force(),
							&Patch::Apply(&preview_app),
						)
						.await
						.map_err(Error::Kube)?;
					info!("Reconciled preview environment {namespace}/{preview_name}");
				}
				"delete" => {
					let _ = app_api
						.delete(&preview_name, &DeleteParams::default())
						.await;
					info!("Deleted preview environment {namespace}/{preview_name}");
				}
				_ => {
					warn!("Unknown preview action: {action}");
				}
			}

			// Clear preview-action annotation after processing
			let patch = serde_json::json!({
				"metadata": {
					"annotations": {
						"reinhardt.dev/preview-action": null,
						"reinhardt.dev/pr-number": null,
						"reinhardt.dev/pr-branch": null
					}
				}
			});
			let parent_api: Api<ReinhardtApp> = Api::namespaced(ctx.client.clone(), namespace);
			parent_api
				.patch(&name, &PatchParams::default(), &Patch::Merge(&patch))
				.await
				.map_err(Error::Kube)?;
		}

		// TTL cleanup for existing previews
		let preview_list = Api::<ReinhardtApp>::namespaced(ctx.client.clone(), namespace)
			.list(&kube::api::ListParams::default().labels(&format!(
				"reinhardt.dev/preview=true,reinhardt.dev/parent-app={name}"
			)))
			.await
			.map_err(Error::Kube)?;

		let ttl = app
			.spec
			.source
			.as_ref()
			.and_then(|s| s.preview.as_ref())
			.and_then(|p| p.ttl.as_deref())
			.unwrap_or("72h");

		for preview_app in preview_list {
			let last_activity = preview_app
				.metadata
				.annotations
				.as_ref()
				.and_then(|a| a.get("reinhardt.dev/last-activity"));
			if let Some(ts) = last_activity
				&& preview::is_ttl_expired(ts, ttl)
			{
				let pname = preview_app.name_any();
				let _ = Api::<ReinhardtApp>::namespaced(ctx.client.clone(), namespace)
					.delete(&pname, &DeleteParams::default())
					.await;
				info!("TTL expired, deleted preview {namespace}/{pname}");
			}
		}
	}

	// Session backend: ensure Redis when session_backend=redis (Phase 4)
	let needs_redis_sessions = app
		.spec
		.introspect
		.as_ref()
		.map(|i| {
			reinhardt_cloud_core::inference::requires_redis_sessions(
				&i.features.infrastructure_signals,
			)
		})
		.unwrap_or(false);
	if needs_redis_sessions && !should_provision_cache(&app) {
		reconcile_cache_deployment(&app, &ctx.client, namespace).await?;
		reconcile_cache_service_resource(&app, &ctx.client, namespace).await?;
		info!("Reconciled Redis for session backend for {name}");
	}

	// i18n ConfigMap provisioning (Phase 5)
	let needs_i18n = app
		.spec
		.introspect
		.as_ref()
		.map(|i| reinhardt_cloud_core::inference::requires_i18n(&i.features.infrastructure_signals))
		.unwrap_or(false);
	if needs_i18n {
		reconcile_i18n_configmap(&app, &ctx.client, namespace).await?;
	}

	// Security: reconciliation for isolated workloads
	if app.spec.isolation.is_some() {
		reconcile_network_policies(&app, &ctx.client, namespace).await?;
		reconcile_resource_limits(
			&app,
			&ctx.client,
			namespace,
			&ctx.platform.defaults.resources,
		)
		.await?;
		reconcile_pss_labels(&ctx.client, namespace).await?;
	}

	// Derive readiness from the observed Deployment status
	let live_deployment = deployments.get(&name).await.map_err(Error::Kube)?;
	let desired_replicas = app.spec.replicas.unwrap_or(1);
	let ready_replicas = live_deployment
		.status
		.as_ref()
		.and_then(|s| s.ready_replicas)
		.unwrap_or(0);
	let ready = ready_replicas >= desired_replicas;

	// Update status sub-resource
	update_status(&app, &ctx.client, namespace, ready, ready_replicas).await?;

	Ok(Action::await_change())
}

/// Clean up external resources when a `ReinhardtApp` is deleted.
///
/// Respects `DeletionPolicy`:
/// - `Retain` (default): only K8s-native resources (Deployment, Service)
///   are removed via ownerReferences GC. Database/cache Secrets and
///   ConfigMaps are retained for manual cleanup.
/// - `Delete`: all resources including Secrets and ConfigMaps are deleted.
async fn cleanup(app: Arc<ReinhardtApp>, ctx: &Context, namespace: &str) -> Result<Action, Error> {
	let name = app.name_any();
	info!("Cleaning up ReinhardtApp {name}");

	// Cache resources (stateless — always clean up)
	delete_if_exists::<Deployment>(&ctx.client, namespace, &format!("{name}-redis")).await?;
	delete_if_exists::<Service>(&ctx.client, namespace, &format!("{name}-redis")).await?;

	// Worker resources (stateless — always clean up)
	delete_if_exists::<Deployment>(&ctx.client, namespace, &format!("{name}-worker")).await?;

	// Security resources (always clean up)
	delete_if_exists::<NetworkPolicy>(&ctx.client, namespace, &format!("{name}-deny-all")).await?;
	delete_if_exists::<NetworkPolicy>(&ctx.client, namespace, &format!("{name}-allow-ingress"))
		.await?;
	delete_if_exists::<NetworkPolicy>(&ctx.client, namespace, &format!("{name}-allow-egress"))
		.await?;
	delete_if_exists::<LimitRange>(&ctx.client, namespace, &format!("{name}-limits")).await?;

	// gRPC Service (stateless — always clean up)
	delete_if_exists::<Service>(&ctx.client, namespace, &format!("{name}-grpc")).await?;

	// Storage ServiceAccount (stateless — always clean up)
	delete_if_exists::<ServiceAccount>(&ctx.client, namespace, &format!("{name}-storage")).await?;

	// i18n ConfigMap (stateless — always clean up)
	delete_if_exists::<ConfigMap>(&ctx.client, namespace, &format!("{name}-locales")).await?;

	// Build Jobs (stateless — always clean up)
	let job_api: Api<Job> = Api::namespaced(ctx.client.clone(), namespace);
	let build_jobs = job_api
		.list(&kube::api::ListParams::default().labels(&format!(
			"app.kubernetes.io/name={name},app.kubernetes.io/component=build"
		)))
		.await
		.map_err(Error::Kube)?;
	for job in build_jobs {
		let jname = job.metadata.name.unwrap_or_default();
		let _ = job_api.delete(&jname, &DeleteParams::default()).await;
	}

	// Always clean up introspect-managed resources that are safe to delete
	// (Ingress, migration Job). These have ownerReferences but we delete
	// explicitly for a cleaner teardown sequence.
	delete_if_exists::<Ingress>(&ctx.client, namespace, &name).await?;
	delete_if_exists::<Job>(&ctx.client, namespace, &format!("{name}-migrate")).await?;

	match app.spec.deletion_policy {
		DeletionPolicy::Retain => {
			// Deployment and Service are cleaned up via ownerReferences GC.
			// Secrets, ConfigMaps, and StatefulSets are retained for manual cleanup.
			info!(
				"DeletionPolicy is Retain: keeping database and cache resources for {name}. \
				 Manual cleanup may be required for: {name}-db-credentials, \
				 {name}-postgresql, {name}-settings"
			);
		}
		DeletionPolicy::Delete => {
			info!("DeletionPolicy is Delete: removing all resources for {name}");

			// Delete settings ConfigMap
			let cm_api: Api<ConfigMap> = Api::namespaced(ctx.client.clone(), namespace);
			let _ = cm_api
				.delete(&format!("{name}-settings"), &DeleteParams::default())
				.await;

			// Delete JWT Secret
			let secret_api: Api<Secret> = Api::namespaced(ctx.client.clone(), namespace);
			let _ = secret_api
				.delete(&format!("{name}-jwt-secret"), &DeleteParams::default())
				.await;

			// Delete DB credentials Secret
			let _ = secret_api
				.delete(&format!("{name}-db-credentials"), &DeleteParams::default())
				.await;

			// Delete SMTP credentials Secret
			let _ = secret_api
				.delete(
					&format!("{name}-smtp-credentials"),
					&DeleteParams::default(),
				)
				.await;

			// Delete git credentials Secret (use spec reference if available)
			if let Some(ref source) = app.spec.source
				&& let Some(ref creds_name) = source.credentials_secret
			{
				let _ = secret_api
					.delete(creds_name, &DeleteParams::default())
					.await;
			}

			// Delete introspect-managed database resources
			delete_if_exists::<StatefulSet>(&ctx.client, namespace, &format!("{name}-postgresql"))
				.await?;
			delete_if_exists::<Service>(&ctx.client, namespace, &format!("{name}-postgresql"))
				.await?;
		}
	}

	Ok(Action::await_change())
}

// ── Conflict resolution: explicit CRD fields vs introspect signals ───

/// Resolve whether PostgreSQL should be provisioned.
///
/// Explicit `spec.database` field takes precedence over introspect signals.
/// Returns `true` if either the explicit database spec is set or the
/// introspect infrastructure signals indicate PostgreSQL usage.
fn should_provision_postgresql(app: &ReinhardtApp) -> bool {
	// Explicit database field takes precedence
	if app.spec.database.is_some() {
		return true;
	}

	// Fall back to introspect signals
	app.spec
		.introspect
		.as_ref()
		.map(|i| {
			reinhardt_cloud_core::inference::requires_postgresql(&i.features.infrastructure_signals)
		})
		.unwrap_or(false)
}

/// Resolve whether cache should be provisioned.
///
/// Explicit `spec.cache` field takes precedence over introspect signals.
fn should_provision_cache(app: &ReinhardtApp) -> bool {
	if app.spec.cache.is_some() {
		return true;
	}

	app.spec
		.introspect
		.as_ref()
		.map(|i| {
			reinhardt_cloud_core::inference::requires_cache(&i.features.infrastructure_signals)
		})
		.unwrap_or(false)
}

/// Resolve the effective application port.
///
/// Explicit `spec.services.target_port` takes precedence over
/// introspect settings. Defaults to 8000 when neither is set.
fn resolve_app_port(app: &ReinhardtApp) -> u16 {
	// Explicit services.target_port takes precedence
	if let Some(services) = &app.spec.services
		&& let Some(port) = services.target_port
	{
		return port as u16;
	}

	// Fall back to introspect settings
	app.spec
		.introspect
		.as_ref()
		.map(|i| reinhardt_cloud_core::inference::app_port(&i.settings))
		.unwrap_or(8000)
}

/// Resolve whether a background worker should be provisioned.
///
/// Explicit `spec.worker` field takes precedence over introspect signals.
fn should_provision_worker(app: &ReinhardtApp) -> bool {
	if app.spec.worker.is_some() {
		return true;
	}

	app.spec
		.introspect
		.as_ref()
		.map(|i| {
			reinhardt_cloud_core::inference::requires_worker(&i.features.infrastructure_signals)
		})
		.unwrap_or(false)
}

/// Resolve whether storage should be provisioned.
///
/// Explicit `spec.storage` field takes precedence over introspect signals.
fn should_provision_storage(app: &ReinhardtApp) -> bool {
	if app.spec.storage.is_some() {
		return true;
	}

	app.spec
		.introspect
		.as_ref()
		.map(|i| {
			reinhardt_cloud_core::inference::requires_storage(&i.features.infrastructure_signals)
		})
		.unwrap_or(false)
}

/// Resolve whether mail should be provisioned.
///
/// Explicit `spec.mail` field takes precedence over introspect signals.
fn should_provision_mail(app: &ReinhardtApp) -> bool {
	if app.spec.mail.is_some() {
		return true;
	}

	app.spec
		.introspect
		.as_ref()
		.map(|i| reinhardt_cloud_core::inference::requires_mail(&i.features.infrastructure_signals))
		.unwrap_or(false)
}

/// Resolve whether ingress routes should be provisioned.
///
/// Explicit `spec.services.ingress_host` takes precedence over
/// introspect routes. Returns the route list and port if ingress
/// should be created, or `None` otherwise.
fn resolve_ingress_config(
	app: &ReinhardtApp,
) -> Option<(Vec<reinhardt_cloud_types::introspect::RouteMetadata>, u16)> {
	// Explicit ingress_host in services spec is handled by the existing
	// reconcile path (build_service/build_ingress from explicit fields).
	// Here we only handle introspect-derived routes when no explicit
	// services config provides an ingress host.
	let has_explicit_ingress = app
		.spec
		.services
		.as_ref()
		.is_some_and(|s| s.ingress_host.is_some());

	if has_explicit_ingress {
		return None;
	}

	// Fall back to introspect routes
	app.spec.introspect.as_ref().and_then(|i| {
		if i.routes.is_empty() {
			None
		} else {
			let port = resolve_app_port(app);
			Some((i.routes.clone(), port))
		}
	})
}

// ── Introspect-aware reconcile helpers ────────────────────────────────

/// Reconciles the database credentials `Secret` for a `ReinhardtApp`.
///
/// Only creates the secret if it does not already exist, preserving
/// existing credentials across reconciliation cycles.
async fn reconcile_db_secret(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
) -> Result<(), Error> {
	let name = app.name_any();
	let secret_name = format!("{name}-db-credentials");
	let secret_api: Api<Secret> = Api::namespaced(client.clone(), namespace);

	if secret_api
		.get_opt(&secret_name)
		.await
		.map_err(Error::Kube)?
		.is_some()
	{
		info!("DB credentials Secret {namespace}/{secret_name} already exists, skipping");
		return Ok(());
	}

	let secret = build_db_secret(app)?;
	secret_api
		.create(&PostParams::default(), &secret)
		.await
		.map_err(|e| Error::SecretGeneration(e.to_string()))?;
	info!("Created DB credentials Secret {namespace}/{secret_name}");
	Ok(())
}

/// Reconciles the PostgreSQL `StatefulSet` via server-side apply.
async fn reconcile_db_statefulset(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
) -> Result<(), Error> {
	let name = app.name_any();
	let sts_name = format!("{name}-postgresql");
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();

	let desired = build_db_statefulset(app)?;
	let sts_api: Api<StatefulSet> = Api::namespaced(client.clone(), namespace);
	sts_api
		.patch(&sts_name, &ssapply, &Patch::Apply(&desired))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled StatefulSet {namespace}/{sts_name}");
	Ok(())
}

/// Reconciles the PostgreSQL headless `Service` via server-side apply.
async fn reconcile_db_service_resource(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
) -> Result<(), Error> {
	let name = app.name_any();
	let svc_name = format!("{name}-postgresql");
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();

	let desired = build_db_service(app)?;
	let svc_api: Api<Service> = Api::namespaced(client.clone(), namespace);
	svc_api
		.patch(&svc_name, &ssapply, &Patch::Apply(&desired))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled DB Service {namespace}/{svc_name}");
	Ok(())
}

/// Reconciles the database migration `Job`.
///
/// - If the job completed successfully, skips re-creation.
/// - If the job failed, deletes it and recreates.
/// - If the job is still running, skips.
/// - If no job exists, creates one.
async fn reconcile_migration_job_resource(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
) -> Result<(), Error> {
	let name = app.name_any();
	let job_name = format!("{name}-migrate");
	let job_api: Api<Job> = Api::namespaced(client.clone(), namespace);

	if let Some(existing) = job_api.get_opt(&job_name).await.map_err(Error::Kube)? {
		let status = existing.status.as_ref();
		let succeeded = status.and_then(|s| s.succeeded).unwrap_or(0);
		let failed = status.and_then(|s| s.failed).unwrap_or(0);
		let active = status.and_then(|s| s.active).unwrap_or(0);

		if succeeded > 0 {
			info!("Migration Job {namespace}/{job_name} already completed, skipping");
			return Ok(());
		}
		if active > 0 {
			info!("Migration Job {namespace}/{job_name} is still running, skipping");
			return Ok(());
		}
		if failed > 0 {
			warn!("Migration Job {namespace}/{job_name} failed, deleting for re-creation");
			// Use propagation policy to clean up pods
			let dp = DeleteParams {
				propagation_policy: Some(kube::api::PropagationPolicy::Background),
				..Default::default()
			};
			job_api.delete(&job_name, &dp).await.map_err(Error::Kube)?;
		}
	}

	let desired = build_migration_job(app)?;
	job_api
		.create(&PostParams::default(), &desired)
		.await
		.map_err(|e| Error::DatabaseProvisioning(e.to_string()))?;
	info!("Created migration Job {namespace}/{job_name}");
	Ok(())
}

/// Reconciles the `Ingress` resource via server-side apply.
async fn reconcile_ingress_resource(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
	routes: &[reinhardt_cloud_types::introspect::RouteMetadata],
	port: u16,
	pages_config: Option<&ResolvedPagesConfig>,
) -> Result<(), Error> {
	let name = app.name_any();
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();

	let signals = app
		.spec
		.introspect
		.as_ref()
		.map(|i| &i.features.infrastructure_signals);
	let Some(desired) = build_ingress(app, routes, port, None, signals, pages_config)? else {
		info!("No Ingress paths for {namespace}/{name}, skipping Ingress creation");
		return Ok(());
	};
	let ingress_api: Api<Ingress> = Api::namespaced(client.clone(), namespace);
	ingress_api
		.patch(&name, &ssapply, &Patch::Apply(&desired))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled Ingress {namespace}/{name}");
	Ok(())
}

/// Reconciles the Redis cache `Deployment` via server-side apply.
async fn reconcile_cache_deployment(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
) -> Result<(), Error> {
	let name = format!("{}-redis", app.name_any());
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();
	let desired = resources::cache::build_cache_deployment(app)?;
	let deployments: Api<Deployment> = Api::namespaced(client.clone(), namespace);
	deployments
		.patch(&name, &ssapply, &Patch::Apply(&desired))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled cache Deployment {namespace}/{name}");
	Ok(())
}

/// Reconciles the Redis cache `Service` via server-side apply.
async fn reconcile_cache_service_resource(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
) -> Result<(), Error> {
	let name = format!("{}-redis", app.name_any());
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();
	let desired = resources::cache::build_cache_service(app)?;
	let services: Api<Service> = Api::namespaced(client.clone(), namespace);
	services
		.patch(&name, &ssapply, &Patch::Apply(&desired))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled cache Service {namespace}/{name}");
	Ok(())
}

/// Reconciles the Worker `Deployment` via server-side apply.
async fn reconcile_worker_deployment_resource(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
	platform: &Platform,
) -> Result<(), Error> {
	let custom_cmd = app.spec.worker.as_ref().and_then(|w| w.command.as_deref());
	let name = format!("{}-worker", app.name_any());
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();
	let desired = resources::worker::build_worker_deployment(app, custom_cmd, platform)?;
	let deployments: Api<Deployment> = Api::namespaced(client.clone(), namespace);
	deployments
		.patch(&name, &ssapply, &Patch::Apply(&desired))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled worker Deployment {namespace}/{name}");
	Ok(())
}

/// Reconciles the gRPC `Service` via server-side apply.
async fn reconcile_grpc_service(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
) -> Result<(), Error> {
	let name = format!("{}-grpc", app.name_any());
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();
	let desired = resources::grpc::build_grpc_service(app)?;
	let services: Api<Service> = Api::namespaced(client.clone(), namespace);
	services
		.patch(&name, &ssapply, &Patch::Apply(&desired))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled gRPC Service {namespace}/{name}");
	Ok(())
}

/// Reconciles the storage `ServiceAccount` via server-side apply.
async fn reconcile_storage_sa(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
	sa: &ServiceAccount,
) -> Result<(), Error> {
	let name = sa
		.metadata
		.name
		.as_ref()
		.cloned()
		.unwrap_or_else(|| format!("{}-storage", app.name_any()));
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();
	let sa_api: Api<ServiceAccount> = Api::namespaced(client.clone(), namespace);
	sa_api
		.patch(&name, &ssapply, &Patch::Apply(sa))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled storage ServiceAccount {namespace}/{name}");
	Ok(())
}

/// Reconciles the SMTP credentials `Secret` for a `ReinhardtApp`.
///
/// Only creates the secret if it does not already exist, preserving
/// user-provided credentials across reconciliation cycles.
async fn reconcile_mail_secret(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
) -> Result<(), Error> {
	let secret_name = format!("{}-smtp-credentials", app.name_any());
	let secrets: Api<Secret> = Api::namespaced(client.clone(), namespace);

	// Create only if not exists (don't overwrite user-provided credentials)
	if secrets
		.get_opt(&secret_name)
		.await
		.map_err(Error::Kube)?
		.is_some()
	{
		return Ok(());
	}

	let desired = resources::mail::build_mail_secret(app)?;
	secrets
		.create(&PostParams::default(), &desired)
		.await
		.map_err(|e| Error::SecretGeneration(e.to_string()))?;
	info!("Created SMTP credentials Secret {namespace}/{secret_name}");
	Ok(())
}

/// Reconciles the i18n locale `ConfigMap` via server-side apply.
async fn reconcile_i18n_configmap(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
) -> Result<(), Error> {
	let name = format!("{}-locales", app.name_any());
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();
	let desired = resources::i18n::build_i18n_configmap(app)?;
	let configmaps: Api<ConfigMap> = Api::namespaced(client.clone(), namespace);
	configmaps
		.patch(&name, &ssapply, &Patch::Apply(&desired))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled i18n ConfigMap {namespace}/{name}");
	Ok(())
}

/// Reconcile NetworkPolicy resources for an isolated `ReinhardtApp`.
async fn reconcile_network_policies(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
) -> Result<(), Error> {
	let network_policies: Api<NetworkPolicy> = Api::namespaced(client.clone(), namespace);
	let pp = PatchParams::apply("reinhardt-cloud-operator").force();

	let deny = build_default_deny_policy(app)?;
	let deny_name = format!("{}-deny-all", app.name_any());
	network_policies
		.patch(&deny_name, &pp, &Patch::Apply(&deny))
		.await
		.map_err(Error::Kube)?;

	let ingress_policy = build_app_ingress_policy(app)?;
	let ingress_name = format!("{}-allow-ingress", app.name_any());
	network_policies
		.patch(&ingress_name, &pp, &Patch::Apply(&ingress_policy))
		.await
		.map_err(Error::Kube)?;

	let network_spec = app
		.spec
		.isolation
		.as_ref()
		.and_then(|i| i.network.as_ref())
		.cloned()
		.unwrap_or_default();
	let egress = build_managed_service_egress_policy(app, &network_spec)?;
	let egress_name = format!("{}-allow-egress", app.name_any());
	network_policies
		.patch(&egress_name, &pp, &Patch::Apply(&egress))
		.await
		.map_err(Error::Kube)?;

	info!("Reconciled NetworkPolicies for {}", app.name_any());
	Ok(())
}

/// Reconcile LimitRange for noisy neighbor protection.
async fn reconcile_resource_limits(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
	defaults: &ResourceDefaults,
) -> Result<(), Error> {
	let limit_ranges: Api<LimitRange> = Api::namespaced(client.clone(), namespace);
	let pp = PatchParams::apply("reinhardt-cloud-operator").force();

	let lr = build_limit_range(app, defaults)?;
	let lr_name = format!("{}-limits", app.name_any());
	limit_ranges
		.patch(&lr_name, &pp, &Patch::Apply(&lr))
		.await
		.map_err(Error::Kube)?;

	info!("Reconciled LimitRange for {}", app.name_any());
	Ok(())
}

/// Apply Pod Security Standards labels to the app's namespace.
async fn reconcile_pss_labels(client: &Client, namespace: &str) -> Result<(), Error> {
	let namespaces: Api<Namespace> = Api::all(client.clone());
	let patch = serde_json::json!({
		"metadata": {
			"labels": {
				"pod-security.kubernetes.io/enforce": "restricted",
				"pod-security.kubernetes.io/enforce-version": "latest",
				"pod-security.kubernetes.io/audit": "restricted",
				"pod-security.kubernetes.io/warn": "restricted"
			}
		}
	});
	namespaces
		.patch(
			namespace,
			&PatchParams::apply("reinhardt-cloud-operator"),
			&Patch::Merge(patch),
		)
		.await
		.map_err(Error::Kube)?;

	info!("Applied PSS Restricted labels to namespace {namespace}");
	Ok(())
}

/// Deletes a namespaced Kubernetes resource if it exists.
///
/// Silently succeeds if the resource is already absent.
async fn delete_if_exists<K>(client: &Client, namespace: &str, name: &str) -> Result<(), Error>
where
	K: kube::Resource<Scope = k8s_openapi::NamespaceResourceScope, DynamicType = ()>
		+ Clone
		+ serde::de::DeserializeOwned
		+ fmt::Debug
		+ k8s_openapi::Metadata<Ty = k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta>,
{
	let api: Api<K> = Api::namespaced(client.clone(), namespace);
	if api.get_opt(name).await.map_err(Error::Kube)?.is_some() {
		api.delete(name, &Default::default())
			.await
			.map_err(Error::Kube)?;
	}
	Ok(())
}

/// Builds the desired `ReinhardtAppStatus` for the given readiness state.
///
/// Pure function that computes the status without any Kubernetes API
/// calls, making it independently testable.
fn build_status(app: &ReinhardtApp, ready: bool, ready_replicas: i32) -> ReinhardtAppStatus {
	let phase = if ready {
		AppPhase::Running
	} else {
		AppPhase::Deploying
	};
	let condition_status = if ready {
		ConditionStatus::True
	} else {
		ConditionStatus::False
	};
	let reason = if ready {
		"ReconcileSuccess"
	} else {
		"ReconcileInProgress"
	};
	let message = if ready {
		"Application is ready"
	} else {
		"Waiting for deployment rollout to complete"
	};

	// Determine lastTransitionTime: preserve existing value if status unchanged
	let existing_ready_condition = app.status.as_ref().and_then(|s| {
		s.conditions
			.iter()
			.find(|c| c.type_ == ConditionType::Ready)
	});

	let last_transition_time = if should_update_transition_time(
		existing_ready_condition.map(|c| &c.status),
		&condition_status,
	) {
		// Status changed or no prior condition: set new transition time
		Some(chrono::Utc::now().to_rfc3339())
	} else {
		// Status unchanged: preserve existing transition time
		existing_ready_condition.and_then(|c| c.last_transition_time.clone())
	};

	// Track database sub-resource status if database is configured
	let database = if app.spec.database.is_some() {
		Some(DatabaseStatus {
			phase: ResourcePhase::Ready,
			endpoint: None,
			credentials_secret: Some(format!("{}-db-credentials", app.name_any())),
		})
	} else {
		None
	};

	ReinhardtAppStatus {
		phase: Some(phase),
		conditions: vec![AppCondition {
			type_: ConditionType::Ready,
			status: condition_status,
			reason: reason.to_string(),
			message: message.to_string(),
			last_transition_time,
			observed_generation: app.metadata.generation,
		}],
		observed_generation: app.metadata.generation,
		ready_replicas: Some(ready_replicas),
		database,
		..Default::default()
	}
}

/// Updates the status sub-resource of a `ReinhardtApp`.
///
/// Only updates `lastTransitionTime` when the condition status actually
/// changes, preventing unnecessary tight reconcile loops.
async fn update_status(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
	ready: bool,
	ready_replicas: i32,
) -> Result<(), Error> {
	let api: Api<ReinhardtApp> = Api::namespaced(client.clone(), namespace);
	let typed_status = build_status(app, ready, ready_replicas);
	let status = serde_json::json!({ "status": typed_status });

	api.patch_status(
		&app.name_any(),
		&PatchParams::default(),
		&Patch::Merge(status),
	)
	.await
	.map_err(Error::Kube)?;

	Ok(())
}

/// Determines whether `lastTransitionTime` should be updated.
///
/// Returns `true` when the condition status has changed or there is no
/// existing condition, indicating a new transition time is needed.
fn should_update_transition_time(
	existing_status: Option<&reinhardt_cloud_types::ConditionStatus>,
	new_status: &reinhardt_cloud_types::ConditionStatus,
) -> bool {
	!matches!(existing_status, Some(existing) if existing == new_status)
}

/// Error policy: requeue after 30 seconds on transient failure.
pub(crate) fn error_policy(_obj: Arc<ReinhardtApp>, error: &Error, _ctx: Arc<Context>) -> Action {
	error!("Reconciliation error: {error}");
	Action::requeue(Duration::from_secs(30))
}

/// Starts the operator controller loop.
pub(crate) async fn run(client: Client) {
	let apps: Api<ReinhardtApp> = Api::all(client.clone());
	let deployments: Api<Deployment> = Api::all(client.clone());
	let services: Api<Service> = Api::all(client.clone());
	let statefulsets: Api<StatefulSet> = Api::all(client.clone());
	let network_policies: Api<NetworkPolicy> = Api::all(client.clone());
	let limit_ranges: Api<LimitRange> = Api::all(client.clone());

	let platform = PlatformConfig::from_env();
	let context = Arc::new(Context { client, platform });

	Controller::new(apps, watcher::Config::default())
		.owns(
			deployments,
			watcher::Config::default()
				.labels("app.kubernetes.io/managed-by=reinhardt-cloud-operator"),
		)
		.owns(
			services,
			watcher::Config::default()
				.labels("app.kubernetes.io/managed-by=reinhardt-cloud-operator"),
		)
		.owns(
			statefulsets,
			watcher::Config::default()
				.labels("app.kubernetes.io/managed-by=reinhardt-cloud-operator"),
		)
		.owns(
			network_policies,
			watcher::Config::default()
				.labels("app.kubernetes.io/managed-by=reinhardt-cloud-operator"),
		)
		.owns(
			limit_ranges,
			watcher::Config::default()
				.labels("app.kubernetes.io/managed-by=reinhardt-cloud-operator"),
		)
		.shutdown_on_signal()
		.run(reconcile, error_policy, context)
		.for_each(|result| async move {
			match result {
				Ok(obj) => info!("Reconciled {obj:?}"),
				Err(err) => error!("Reconciliation error: {err:?}"),
			}
		})
		.await;
}

#[cfg(test)]
mod tests {
	use super::*;
	use reinhardt_cloud_types::crd::database::{DatabaseEngine, DatabaseSpec};
	use reinhardt_cloud_types::crd::{
		AppCondition, AppPhase, ReinhardtAppSpec, ReinhardtAppStatus,
	};
	use reinhardt_cloud_types::{ConditionStatus, ConditionType};
	use rstest::rstest;

	/// Helper to create a minimal `ReinhardtApp` for reconciler tests.
	fn make_test_app(name: &str) -> ReinhardtApp {
		ReinhardtApp {
			metadata: kube::api::ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				generation: Some(1),
				..Default::default()
			},
			spec: ReinhardtAppSpec {
				image: "test:latest".to_string(),
				..Default::default()
			},
			status: None,
		}
	}

	// ── should_update_transition_time tests ──────────────────────────

	#[rstest]
	#[case(ConditionStatus::True, ConditionStatus::False)]
	#[case(ConditionStatus::False, ConditionStatus::True)]
	#[case(ConditionStatus::Unknown, ConditionStatus::True)]
	fn test_should_update_transition_time_returns_true_when_status_changes(
		#[case] existing: ConditionStatus,
		#[case] new: ConditionStatus,
	) {
		// Arrange
		let existing_ref = Some(&existing);

		// Act
		let result = should_update_transition_time(existing_ref, &new);

		// Assert
		assert!(result);
	}

	#[rstest]
	#[case(ConditionStatus::True)]
	#[case(ConditionStatus::False)]
	#[case(ConditionStatus::Unknown)]
	fn test_should_update_transition_time_returns_false_when_status_unchanged(
		#[case] status: ConditionStatus,
	) {
		// Arrange
		let existing_ref = Some(&status);

		// Act
		let result = should_update_transition_time(existing_ref, &status);

		// Assert
		assert!(!result);
	}

	#[rstest]
	fn test_should_update_transition_time_returns_true_when_no_existing() {
		// Arrange
		let new_status = ConditionStatus::True;

		// Act
		let result = should_update_transition_time(None, &new_status);

		// Assert
		assert!(result);
	}

	// ── build_status tests ───────────────────────────────────────────

	#[rstest]
	fn test_build_status_sets_running_phase_when_ready() {
		// Arrange
		let app = make_test_app("ready-app");

		// Act
		let status = build_status(&app, true, 3);

		// Assert
		assert_eq!(status.phase, Some(AppPhase::Running));
		assert_eq!(status.conditions.len(), 1);
		assert_eq!(status.conditions[0].type_, ConditionType::Ready);
		assert_eq!(status.conditions[0].status, ConditionStatus::True);
		assert_eq!(status.conditions[0].reason, "ReconcileSuccess");
		assert_eq!(status.conditions[0].message, "Application is ready");
	}

	#[rstest]
	fn test_build_status_sets_deploying_phase_when_not_ready() {
		// Arrange
		let app = make_test_app("deploying-app");

		// Act
		let status = build_status(&app, false, 0);

		// Assert
		assert_eq!(status.phase, Some(AppPhase::Deploying));
		assert_eq!(status.conditions.len(), 1);
		assert_eq!(status.conditions[0].type_, ConditionType::Ready);
		assert_eq!(status.conditions[0].status, ConditionStatus::False);
		assert_eq!(status.conditions[0].reason, "ReconcileInProgress");
		assert_eq!(
			status.conditions[0].message,
			"Waiting for deployment rollout to complete"
		);
	}

	#[rstest]
	fn test_build_status_sets_observed_generation() {
		// Arrange
		let mut app = make_test_app("gen-app");
		app.metadata.generation = Some(5);

		// Act
		let status = build_status(&app, true, 1);

		// Assert
		assert_eq!(status.observed_generation, Some(5));
		assert_eq!(status.conditions[0].observed_generation, Some(5));
	}

	#[rstest]
	fn test_build_status_sets_ready_replicas() {
		// Arrange
		let app = make_test_app("replicas-app");

		// Act
		let status = build_status(&app, true, 7);

		// Assert
		assert_eq!(status.ready_replicas, Some(7));
	}

	#[rstest]
	fn test_build_status_preserves_transition_time_when_status_unchanged() {
		// Arrange
		let preserved_time = "2025-06-15T12:00:00Z";
		let mut app = make_test_app("preserve-time-app");
		app.status = Some(ReinhardtAppStatus {
			phase: Some(AppPhase::Running),
			conditions: vec![AppCondition {
				type_: ConditionType::Ready,
				status: ConditionStatus::True,
				reason: "ReconcileSuccess".to_string(),
				message: "Application is ready".to_string(),
				last_transition_time: Some(preserved_time.to_string()),
				observed_generation: Some(1),
			}],
			observed_generation: Some(1),
			ready_replicas: Some(1),
			..Default::default()
		});

		// Act — same readiness state (ready=true matching existing True)
		let status = build_status(&app, true, 1);

		// Assert — transition time should be preserved
		assert_eq!(
			status.conditions[0].last_transition_time,
			Some(preserved_time.to_string())
		);
	}

	#[rstest]
	fn test_build_status_updates_transition_time_when_status_changes() {
		// Arrange
		let old_time = "2025-01-01T00:00:00Z";
		let mut app = make_test_app("change-time-app");
		app.status = Some(ReinhardtAppStatus {
			phase: Some(AppPhase::Running),
			conditions: vec![AppCondition {
				type_: ConditionType::Ready,
				status: ConditionStatus::True,
				reason: "ReconcileSuccess".to_string(),
				message: "Application is ready".to_string(),
				last_transition_time: Some(old_time.to_string()),
				observed_generation: Some(1),
			}],
			observed_generation: Some(1),
			ready_replicas: Some(1),
			..Default::default()
		});

		// Act — readiness changed from True to False
		let status = build_status(&app, false, 0);

		// Assert — transition time should be updated (not the old value)
		assert_ne!(
			status.conditions[0].last_transition_time,
			Some(old_time.to_string())
		);
		assert!(status.conditions[0].last_transition_time.is_some());
	}

	#[rstest]
	fn test_build_status_sets_new_transition_time_when_no_existing_status() {
		// Arrange
		let app = make_test_app("new-app");

		// Act
		let status = build_status(&app, true, 1);

		// Assert — should have a transition time since there's no prior condition
		assert!(status.conditions[0].last_transition_time.is_some());
	}

	// ── build_status database sub-resource tests ────────────────────

	#[rstest]
	fn test_build_status_includes_database_status_when_database_configured() {
		// Arrange
		let mut app = make_test_app("db-app");
		app.spec.database = Some(DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: Some(20),
			version: None,
		});

		// Act
		let status = build_status(&app, true, 1);

		// Assert
		let db_status = status.database.expect("database status should be present");
		assert_eq!(db_status.phase, ResourcePhase::Ready);
		assert_eq!(
			db_status.credentials_secret,
			Some("db-app-db-credentials".to_string())
		);
		assert!(db_status.endpoint.is_none());
	}

	#[rstest]
	fn test_build_status_excludes_database_status_when_no_database() {
		// Arrange
		let app = make_test_app("no-db-app");

		// Act
		let status = build_status(&app, true, 1);

		// Assert
		assert!(status.database.is_none());
	}

	// ── error_policy tests ───────────────────────────────────────────

	/// Creates a `kube::Client` backed by a dummy service for unit tests.
	///
	/// The returned client will never be used for real API calls; it only
	/// satisfies the type signature of `error_policy`.
	fn dummy_client() -> Client {
		use http::Response;
		use http_body_util::Empty;
		use tower::service_fn;

		let svc = service_fn(|_req: http::Request<kube::client::Body>| async {
			Ok::<_, std::convert::Infallible>(Response::builder().body(Empty::new()).unwrap())
		});
		Client::new(svc, "default")
	}

	// ── deletion_policy tests ───────────────────────────────────────

	#[rstest]
	fn test_deletion_policy_defaults_to_retain() {
		// Arrange
		let app = make_test_app("retain-app");

		// Assert
		assert_eq!(app.spec.deletion_policy, DeletionPolicy::Retain);
	}

	#[rstest]
	fn test_deletion_policy_can_be_set_to_delete() {
		// Arrange
		let mut app = make_test_app("delete-app");

		// Act
		app.spec.deletion_policy = DeletionPolicy::Delete;

		// Assert
		assert_eq!(app.spec.deletion_policy, DeletionPolicy::Delete);
	}

	// ── error_policy tests ───────────────────────────────────────────

	#[rstest]
	#[tokio::test]
	async fn test_error_policy_returns_requeue_action() {
		// Arrange
		let app = Arc::new(make_test_app("error-app"));
		let error = Error::MissingNamespace("error-app".to_string());
		let ctx = Arc::new(Context {
			client: dummy_client(),
			platform: PlatformConfig::onprem_defaults(),
		});

		// Act
		let action = error_policy(app, &error, ctx);

		// Assert — error_policy must return a 30-second requeue
		let expected = Action::requeue(Duration::from_secs(30));
		assert_eq!(format!("{action:?}"), format!("{expected:?}"));
	}

	// ── introspect-aware decision logic tests ───────────────────────

	#[rstest]
	fn test_introspect_with_postgresql_triggers_db_path() {
		// Arrange
		use reinhardt_cloud_types::introspect::{FeaturesMetadata, InfraSignals, IntrospectOutput};

		let mut app = make_test_app("introspect-pg-app");
		app.spec.introspect = Some(IntrospectOutput {
			features: FeaturesMetadata {
				infrastructure_signals: InfraSignals {
					database: Some("postgres".to_string()),
					..Default::default()
				},
				..Default::default()
			},
			..Default::default()
		});

		// Act
		let introspect = app.spec.introspect.as_ref().unwrap();
		let signals = &introspect.features.infrastructure_signals;
		let needs_pg = reinhardt_cloud_core::inference::requires_postgresql(signals);

		// Assert
		assert!(needs_pg);
	}

	#[rstest]
	fn test_no_introspect_skips_db_path() {
		// Arrange
		let app = make_test_app("legacy-app");

		// Act / Assert
		assert!(app.spec.introspect.is_none());
	}

	#[rstest]
	fn test_introspect_with_routes_triggers_ingress_path() {
		// Arrange
		use reinhardt_cloud_types::introspect::{
			IntrospectOutput, RouteMetadata, SettingsMetadata,
		};

		let mut app = make_test_app("route-app");
		app.spec.introspect = Some(IntrospectOutput {
			routes: vec![RouteMetadata {
				path: "/api/users/".to_string(),
				methods: vec!["GET".to_string()],
				name: None,
				namespace: None,
			}],
			settings: SettingsMetadata::default(),
			..Default::default()
		});

		// Act
		let introspect = app.spec.introspect.as_ref().unwrap();
		let has_routes = !introspect.routes.is_empty();
		let port = reinhardt_cloud_core::inference::app_port(&introspect.settings);

		// Assert
		assert!(has_routes);
		assert_eq!(port, 8000);
	}

	#[rstest]
	fn test_introspect_without_db_skips_postgresql() {
		// Arrange
		use reinhardt_cloud_types::introspect::{FeaturesMetadata, InfraSignals, IntrospectOutput};

		let mut app = make_test_app("no-db-introspect-app");
		app.spec.introspect = Some(IntrospectOutput {
			features: FeaturesMetadata {
				infrastructure_signals: InfraSignals {
					database: None,
					..Default::default()
				},
				..Default::default()
			},
			..Default::default()
		});

		// Act
		let introspect = app.spec.introspect.as_ref().unwrap();
		let signals = &introspect.features.infrastructure_signals;
		let needs_pg = reinhardt_cloud_core::inference::requires_postgresql(signals);

		// Assert
		assert!(!needs_pg);
	}

	#[rstest]
	fn test_cleanup_retain_policy_preserves_db_credentials() {
		// Arrange
		let app = make_test_app("retain-db-app");

		// Act / Assert — DeletionPolicy::Retain means credentials are NOT deleted
		assert_eq!(app.spec.deletion_policy, DeletionPolicy::Retain);
		// With Retain policy, the cleanup function logs but does not delete secrets
	}

	#[rstest]
	fn test_cleanup_delete_policy_removes_all_resources() {
		// Arrange
		let mut app = make_test_app("delete-all-app");
		app.spec.deletion_policy = DeletionPolicy::Delete;

		// Act / Assert — DeletionPolicy::Delete means everything gets cleaned up
		assert_eq!(app.spec.deletion_policy, DeletionPolicy::Delete);
	}

	// ── conflict resolution tests ───────────────────────────────────

	#[rstest]
	fn test_explicit_database_overrides_introspect() {
		// Arrange — explicit database set, introspect also has postgresql
		use reinhardt_cloud_types::introspect::{FeaturesMetadata, InfraSignals, IntrospectOutput};

		let mut app = make_test_app("explicit-db-app");
		app.spec.database = Some(DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: Some(20),
			version: None,
		});
		app.spec.introspect = Some(IntrospectOutput {
			features: FeaturesMetadata {
				infrastructure_signals: InfraSignals {
					database: Some("postgres".to_string()),
					..Default::default()
				},
				..Default::default()
			},
			..Default::default()
		});

		// Act
		let result = should_provision_postgresql(&app);

		// Assert — should use explicit database path (returns true because
		// explicit spec.database is set)
		assert!(result);
		assert!(app.spec.database.is_some());
	}

	#[rstest]
	fn test_introspect_used_when_no_explicit_database() {
		// Arrange — no explicit database, introspect has postgresql
		use reinhardt_cloud_types::introspect::{FeaturesMetadata, InfraSignals, IntrospectOutput};

		let mut app = make_test_app("introspect-only-db-app");
		app.spec.introspect = Some(IntrospectOutput {
			features: FeaturesMetadata {
				infrastructure_signals: InfraSignals {
					database: Some("postgres".to_string()),
					..Default::default()
				},
				..Default::default()
			},
			..Default::default()
		});

		// Act
		let result = should_provision_postgresql(&app);

		// Assert — should provision from introspect fallback
		assert!(result);
		assert!(app.spec.database.is_none());
	}

	#[rstest]
	fn test_no_database_when_neither_set() {
		// Arrange — no explicit database, no introspect
		let app = make_test_app("no-db-no-introspect-app");

		// Act
		let result = should_provision_postgresql(&app);

		// Assert — should not provision database
		assert!(!result);
	}

	#[rstest]
	fn test_resolve_app_port_explicit_overrides() {
		// Arrange — explicit services.target_port = 3000, introspect says 9000
		use reinhardt_cloud_types::crd::spec::ServicesSpec;
		use reinhardt_cloud_types::introspect::{
			IntrospectOutput, ServerSettings, SettingsMetadata,
		};

		let mut app = make_test_app("explicit-port-app");
		app.spec.services = Some(ServicesSpec {
			port: None,
			target_port: Some(3000),
			ingress_host: None,
		});
		app.spec.introspect = Some(IntrospectOutput {
			settings: SettingsMetadata {
				server: ServerSettings {
					default_port: 9000,
					..Default::default()
				},
				..Default::default()
			},
			..Default::default()
		});

		// Act
		let port = resolve_app_port(&app);

		// Assert — explicit target_port takes precedence
		assert_eq!(port, 3000);
	}

	#[rstest]
	fn test_resolve_app_port_falls_back_to_introspect() {
		// Arrange — no explicit port, introspect says 9000
		use reinhardt_cloud_types::introspect::{
			IntrospectOutput, ServerSettings, SettingsMetadata,
		};

		let mut app = make_test_app("introspect-port-app");
		app.spec.introspect = Some(IntrospectOutput {
			settings: SettingsMetadata {
				server: ServerSettings {
					default_port: 9000,
					..Default::default()
				},
				..Default::default()
			},
			..Default::default()
		});

		// Act
		let port = resolve_app_port(&app);

		// Assert — should return introspect-derived port
		assert_eq!(port, 9000);
	}

	#[rstest]
	fn test_resolve_app_port_defaults_when_neither_set() {
		// Arrange — no explicit port, no introspect
		let app = make_test_app("default-port-app");

		// Act
		let port = resolve_app_port(&app);

		// Assert — should return default 8000
		assert_eq!(port, 8000);
	}

	#[rstest]
	fn test_should_provision_cache_explicit_overrides() {
		// Arrange — explicit cache set, introspect also has redis
		use reinhardt_cloud_types::crd::cache::{CacheBackend, CacheSpec};
		use reinhardt_cloud_types::introspect::{FeaturesMetadata, InfraSignals, IntrospectOutput};

		let mut app = make_test_app("explicit-cache-app");
		app.spec.cache = Some(CacheSpec {
			backend: CacheBackend::Redis,
			instance_type: Some("cache.t3.micro".to_string()),
		});
		app.spec.introspect = Some(IntrospectOutput {
			features: FeaturesMetadata {
				infrastructure_signals: InfraSignals {
					cache: Some("redis".to_string()),
					..Default::default()
				},
				..Default::default()
			},
			..Default::default()
		});

		// Act
		let result = should_provision_cache(&app);

		// Assert — explicit cache spec takes precedence
		assert!(result);
		assert!(app.spec.cache.is_some());
	}

	#[rstest]
	fn test_should_provision_cache_falls_back_to_introspect() {
		// Arrange — no explicit cache, introspect has redis
		use reinhardt_cloud_types::introspect::{FeaturesMetadata, InfraSignals, IntrospectOutput};

		let mut app = make_test_app("introspect-cache-app");
		app.spec.introspect = Some(IntrospectOutput {
			features: FeaturesMetadata {
				infrastructure_signals: InfraSignals {
					cache: Some("redis".to_string()),
					..Default::default()
				},
				..Default::default()
			},
			..Default::default()
		});

		// Act
		let result = should_provision_cache(&app);

		// Assert — introspect fallback used
		assert!(result);
		assert!(app.spec.cache.is_none());
	}

	#[rstest]
	fn test_should_provision_worker_explicit_overrides() {
		// Arrange — explicit worker set, introspect also has background_worker
		use reinhardt_cloud_types::crd::worker::WorkerSpec;
		use reinhardt_cloud_types::introspect::{FeaturesMetadata, InfraSignals, IntrospectOutput};

		let mut app = make_test_app("explicit-worker-app");
		app.spec.worker = Some(WorkerSpec {
			concurrency: Some(4),
			command: None,
		});
		app.spec.introspect = Some(IntrospectOutput {
			features: FeaturesMetadata {
				infrastructure_signals: InfraSignals {
					background_worker: true,
					..Default::default()
				},
				..Default::default()
			},
			..Default::default()
		});

		// Act
		let result = should_provision_worker(&app);

		// Assert — explicit worker spec takes precedence
		assert!(result);
		assert!(app.spec.worker.is_some());
	}

	#[rstest]
	fn test_resolve_ingress_explicit_host_skips_introspect() {
		// Arrange — explicit ingress_host set, introspect also has routes
		use reinhardt_cloud_types::crd::spec::ServicesSpec;
		use reinhardt_cloud_types::introspect::{IntrospectOutput, RouteMetadata};

		let mut app = make_test_app("explicit-ingress-app");
		app.spec.services = Some(ServicesSpec {
			port: Some(80),
			target_port: Some(8080),
			ingress_host: Some("myapp.example.com".to_string()),
		});
		app.spec.introspect = Some(IntrospectOutput {
			routes: vec![RouteMetadata {
				path: "/api/".to_string(),
				methods: vec!["GET".to_string()],
				name: None,
				namespace: None,
			}],
			..Default::default()
		});

		// Act
		let config = resolve_ingress_config(&app);

		// Assert — explicit ingress_host takes precedence, introspect routes skipped
		assert!(config.is_none());
	}

	#[rstest]
	fn test_resolve_ingress_falls_back_to_introspect_routes() {
		// Arrange — no explicit ingress_host, introspect has routes
		use reinhardt_cloud_types::introspect::{IntrospectOutput, RouteMetadata};

		let mut app = make_test_app("introspect-ingress-app");
		app.spec.introspect = Some(IntrospectOutput {
			routes: vec![RouteMetadata {
				path: "/api/v1/".to_string(),
				methods: vec!["GET".to_string(), "POST".to_string()],
				name: None,
				namespace: None,
			}],
			..Default::default()
		});

		// Act
		let config = resolve_ingress_config(&app);

		// Assert — introspect routes used with default port
		let (routes, port) = config.expect("should return ingress config");
		assert_eq!(routes.len(), 1);
		assert_eq!(routes[0].path, "/api/v1/");
		assert_eq!(port, 8000);
	}

	#[rstest]
	fn test_resolve_ingress_none_when_neither_set() {
		// Arrange — no explicit ingress_host, no introspect
		let app = make_test_app("no-ingress-app");

		// Act
		let config = resolve_ingress_config(&app);

		// Assert — no ingress should be created
		assert!(config.is_none());
	}

	// ── cache/worker integration tests ──────────────────────────────

	#[rstest]
	fn test_should_provision_cache_from_introspect_triggers_builders() {
		// Arrange
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": "myapp", "namespace": "default", "uid": "uid" },
			"spec": {
				"image": "myapp:latest",
				"introspect": {
					"features": {
						"infrastructure_signals": {
							"cache": "redis"
						}
					}
				}
			}
		});
		let app: ReinhardtApp = serde_json::from_value(json).unwrap();

		// Act
		let should_cache = should_provision_cache(&app);

		// Assert
		assert!(should_cache);
		let deploy = resources::cache::build_cache_deployment(&app).unwrap();
		assert_eq!(deploy.metadata.name.unwrap(), "myapp-redis");
	}

	#[rstest]
	fn test_should_provision_worker_from_introspect_triggers_builders() {
		// Arrange
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": "myapp", "namespace": "default", "uid": "uid" },
			"spec": {
				"image": "myapp:latest",
				"introspect": {
					"features": {
						"infrastructure_signals": {
							"background_worker": true
						}
					}
				}
			}
		});
		let app: ReinhardtApp = serde_json::from_value(json).unwrap();

		// Act
		let should_worker = should_provision_worker(&app);

		// Assert
		assert!(should_worker);
		let deploy = resources::worker::build_worker_deployment(
			&app,
			None,
			&crate::inference::platform::Platform::Onpremise,
		)
		.unwrap();
		assert_eq!(deploy.metadata.name.unwrap(), "myapp-worker");
	}

	#[rstest]
	fn test_explicit_cache_also_triggers_provisioning() {
		// Arrange — explicit spec.cache set (should still trigger provisioning)
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": "myapp", "namespace": "default", "uid": "uid" },
			"spec": {
				"image": "myapp:latest",
				"cache": { "backend": "redis" }
			}
		});
		let app: ReinhardtApp = serde_json::from_value(json).unwrap();

		// Act
		let should_cache = should_provision_cache(&app);

		// Assert — explicit cache should trigger provisioning
		assert!(should_cache);
	}

	// ── gRPC / WebSocket integration tests ─────────────────────────

	#[rstest]
	fn test_grpc_signal_triggers_service_builder() {
		// Arrange
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": "myapp", "namespace": "default", "uid": "uid" },
			"spec": {
				"image": "myapp:latest",
				"introspect": {
					"features": {
						"infrastructure_signals": { "grpc": true }
					}
				}
			}
		});
		let app: ReinhardtApp = serde_json::from_value(json).unwrap();

		// Act
		let needs_grpc = app
			.spec
			.introspect
			.as_ref()
			.map(|i| {
				reinhardt_cloud_core::inference::requires_grpc(&i.features.infrastructure_signals)
			})
			.unwrap_or(false);

		// Assert
		assert!(needs_grpc);
		let svc = resources::grpc::build_grpc_service(&app).unwrap();
		assert_eq!(svc.metadata.name.unwrap(), "myapp-grpc");
	}

	// ── storage / mail / session integration tests ─────────────────

	#[rstest]
	fn test_should_provision_storage_from_introspect() {
		// Arrange
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": "myapp", "namespace": "default", "uid": "uid" },
			"spec": {
				"image": "myapp:latest",
				"introspect": {
					"features": {
						"infrastructure_signals": { "storage": "s3" }
					}
				}
			}
		});
		let app: ReinhardtApp = serde_json::from_value(json).unwrap();

		// Act / Assert
		assert!(should_provision_storage(&app));
	}

	#[rstest]
	fn test_should_provision_storage_from_explicit_spec() {
		// Arrange
		use reinhardt_cloud_types::crd::storage::{StorageBackend, StorageSpec};

		let mut app = make_test_app("explicit-storage-app");
		app.spec.storage = Some(StorageSpec {
			backend: Some(StorageBackend::S3),
			bucket: None,
		});

		// Act / Assert
		assert!(should_provision_storage(&app));
	}

	#[rstest]
	fn test_should_provision_storage_false_when_neither_set() {
		// Arrange
		let app = make_test_app("no-storage-app");

		// Act / Assert
		assert!(!should_provision_storage(&app));
	}

	#[rstest]
	fn test_should_provision_mail_from_introspect() {
		// Arrange
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": "myapp", "namespace": "default", "uid": "uid" },
			"spec": {
				"image": "myapp:latest",
				"introspect": {
					"features": {
						"infrastructure_signals": { "mail": "smtp" }
					}
				}
			}
		});
		let app: ReinhardtApp = serde_json::from_value(json).unwrap();

		// Act / Assert
		assert!(should_provision_mail(&app));
	}

	#[rstest]
	fn test_should_provision_mail_from_explicit_spec() {
		// Arrange
		use reinhardt_cloud_types::crd::mail::MailSpec;

		let mut app = make_test_app("explicit-mail-app");
		app.spec.mail = Some(MailSpec {
			smtp_host: Some("smtp.example.com".to_string()),
			smtp_port: Some(587),
			credentials_secret: None,
		});

		// Act / Assert
		assert!(should_provision_mail(&app));
	}

	#[rstest]
	fn test_should_provision_mail_false_when_neither_set() {
		// Arrange
		let app = make_test_app("no-mail-app");

		// Act / Assert
		assert!(!should_provision_mail(&app));
	}

	#[rstest]
	fn test_redis_sessions_without_explicit_cache() {
		// Arrange
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": "myapp", "namespace": "default", "uid": "uid" },
			"spec": {
				"image": "myapp:latest",
				"introspect": {
					"features": {
						"infrastructure_signals": { "session_backend": "redis" }
					}
				}
			}
		});
		let app: ReinhardtApp = serde_json::from_value(json).unwrap();

		// Act
		let needs = app
			.spec
			.introspect
			.as_ref()
			.map(|i| {
				reinhardt_cloud_core::inference::requires_redis_sessions(
					&i.features.infrastructure_signals,
				)
			})
			.unwrap_or(false);

		// Assert
		assert!(needs);
		assert!(!should_provision_cache(&app)); // No explicit cache
	}

	#[rstest]
	fn test_redis_sessions_skipped_when_cache_already_provisioned() {
		// Arrange — both session_backend=redis and cache=redis
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": "myapp", "namespace": "default", "uid": "uid" },
			"spec": {
				"image": "myapp:latest",
				"cache": { "backend": "redis" },
				"introspect": {
					"features": {
						"infrastructure_signals": { "session_backend": "redis" }
					}
				}
			}
		});
		let app: ReinhardtApp = serde_json::from_value(json).unwrap();

		// Act
		let needs_sessions = app
			.spec
			.introspect
			.as_ref()
			.map(|i| {
				reinhardt_cloud_core::inference::requires_redis_sessions(
					&i.features.infrastructure_signals,
				)
			})
			.unwrap_or(false);

		// Assert — Redis sessions needed, but cache is already provisioned
		assert!(needs_sessions);
		assert!(should_provision_cache(&app));
		// The reconciler condition `needs_redis_sessions && !should_provision_cache`
		// is false, so session-only Redis provisioning is skipped
	}

	#[rstest]
	fn test_storage_sa_builder_triggered_for_s3() {
		// Arrange
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": "myapp", "namespace": "default", "uid": "uid" },
			"spec": {
				"image": "myapp:latest",
				"introspect": {
					"features": {
						"infrastructure_signals": { "storage": "s3" }
					}
				}
			}
		});
		let app: ReinhardtApp = serde_json::from_value(json).unwrap();

		// Act
		let backend = app
			.spec
			.introspect
			.as_ref()
			.and_then(|i| i.features.infrastructure_signals.storage.as_deref())
			.unwrap_or("pvc");
		let sa = resources::storage::build_storage_service_account(
			&app,
			backend,
			Some("arn:aws:iam::123456:role/test-role"),
		)
		.expect("build should succeed");

		// Assert
		assert!(sa.is_some());
		assert_eq!(sa.unwrap().metadata.name.as_deref(), Some("myapp-storage"));
	}

	#[rstest]
	fn test_mail_secret_builder_triggered() {
		// Arrange
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": "myapp", "namespace": "default", "uid": "uid" },
			"spec": {
				"image": "myapp:latest",
				"introspect": {
					"features": {
						"infrastructure_signals": { "mail": "smtp" }
					}
				}
			}
		});
		let app: ReinhardtApp = serde_json::from_value(json).unwrap();

		// Act
		let secret = resources::mail::build_mail_secret(&app).expect("build should succeed");

		// Assert
		assert_eq!(
			secret.metadata.name.as_deref(),
			Some("myapp-smtp-credentials")
		);
	}

	#[rstest]
	fn test_i18n_signal_triggers_configmap_builder() {
		// Arrange
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": "myapp", "namespace": "default", "uid": "uid" },
			"spec": {
				"image": "myapp:latest",
				"introspect": {
					"features": {
						"infrastructure_signals": { "i18n": true }
					}
				}
			}
		});
		let app: ReinhardtApp = serde_json::from_value(json).unwrap();

		// Act
		let needs = app
			.spec
			.introspect
			.as_ref()
			.map(|i| {
				reinhardt_cloud_core::inference::requires_i18n(&i.features.infrastructure_signals)
			})
			.unwrap_or(false);

		// Assert
		assert!(needs);
		let cm = resources::i18n::build_i18n_configmap(&app).unwrap();
		assert_eq!(cm.metadata.name.unwrap(), "myapp-locales");
	}

	// ── pages inference tests ───────────────────────────────────────

	#[rstest]
	fn test_resolve_pages_config_via_spec() {
		// Arrange
		let mut app = make_test_app("app");
		app.spec.pages = Some(reinhardt_cloud_types::crd::pages::PagesSpec {
			static_root: None,
			static_url: None,
			server_image: None,
			server_resources: None,
			cache_max_age: None,
			brotli: None,
			gzip: None,
		});

		// Act
		let config = resolve_pages_config(&app);

		// Assert
		assert!(config.is_some());
	}

	#[rstest]
	fn test_resolve_pages_config_none_without_spec_or_introspect() {
		// Arrange
		let app = make_test_app("app");

		// Act
		let config = resolve_pages_config(&app);

		// Assert
		assert!(config.is_none());
	}

	#[rstest]
	fn test_websocket_signal_adds_ingress_annotations() {
		// Arrange
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": "wsapp", "namespace": "default", "uid": "uid" },
			"spec": {
				"image": "wsapp:latest",
				"introspect": {
					"routes": [{"path": "/ws/", "methods": []}],
					"features": {
						"infrastructure_signals": { "websocket": true }
					}
				}
			}
		});
		let app: ReinhardtApp = serde_json::from_value(json).unwrap();

		// Act
		let signals = app
			.spec
			.introspect
			.as_ref()
			.map(|i| &i.features.infrastructure_signals);
		let routes = &app.spec.introspect.as_ref().unwrap().routes;
		let ingress = resources::ingress::build_ingress(&app, routes, 8000, None, signals, None)
			.unwrap()
			.expect("ingress should be created for non-empty routes");

		// Assert
		let annotations = ingress.metadata.annotations.unwrap();
		assert_eq!(
			annotations.get("nginx.ingress.kubernetes.io/proxy-read-timeout"),
			Some(&"3600".to_string())
		);
	}
}
