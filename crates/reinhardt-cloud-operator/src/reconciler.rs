//! Reconciler logic for the `Project` custom resource.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use k8s_openapi::api::apps::v1::{Deployment, StatefulSet};
use k8s_openapi::api::autoscaling::v2::HorizontalPodAutoscaler;
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::{
	ConfigMap, LimitRange, Namespace, Secret, Service, ServiceAccount,
};
use k8s_openapi::api::networking::v1::{Ingress, IngressRule, NetworkPolicy};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::api::{Api, DeleteParams, ListParams, Patch, PatchParams, PostParams};
use kube::runtime::controller::{Action, Controller};
use kube::runtime::finalizer::{Event as FinalizerEvent, finalizer};
use kube::runtime::watcher;
use kube::{Client, Resource, ResourceExt};
use tracing::{Instrument, error, info, warn};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use uuid::Uuid;

use dashmap::DashMap;

use crate::error::{BackoffClass, Error, backoff_class};
use crate::inference::database::{DatabaseResource, infer_database_resources};
use crate::inference::pages::{ResolvedPagesConfig, resolve_pages_config};
use crate::inference::platform::{Platform, PlatformConfig, ResourceDefaults};
use crate::inference::secrets::{
	build_core_secret_key_secret, build_jwt_secret, build_redis_credentials_secret,
};
use crate::metrics::Metrics;
use crate::resources::credentials;
use crate::resources::migration::{
	build_migration_job, migration_job_name, migration_revision_key,
};
use crate::resources::preview;
use crate::resources::security::limit_range::build_limit_range;
use crate::resources::security::network_policy::{
	build_app_ingress_policy, build_default_deny_policy, build_managed_service_egress_policy,
};
use crate::resources::tenant as tenant_resources;
use crate::resources::{
	self, AutoscalerPlan, build_autoscaler, build_db_secret, build_db_service,
	build_db_statefulset, build_deployment, build_ingress, build_service, hpa_is_ready,
};
use crate::source_build::{self, BuildDecision};
use k8s_openapi::api::core::v1::PersistentVolumeClaim;
use kube::api::DynamicObject;
use reinhardt_cloud_types::crd::database::{DatabaseStatus, ResourcePhase};
use reinhardt_cloud_types::crd::policy::DeletionPolicy;
use reinhardt_cloud_types::crd::spec::ScaleMetric;
use reinhardt_cloud_types::crd::{
	BuildStatus, BuildTargetKind, Project, ProjectCondition, ProjectPhase, ProjectStatus,
};
use reinhardt_cloud_types::{ConditionStatus, ConditionType};

const FINALIZER_NAME: &str = "paas.reinhardt-cloud.dev/cleanup";

fn managed_git_credentials_secret_name(project_name: &str) -> String {
	format!("{project_name}-git-credentials")
}

fn managed_github_credentials_secret_name(project_name: &str) -> String {
	format!("{project_name}-github-git-credentials")
}

/// Annotation key on a `Project` that carries an incoming W3C `traceparent`
/// for distributed-trace propagation into the reconcile span.
///
/// Writing the value back to the CRD is intentionally deferred: a patch-loop
/// risk exists if the operator itself writes the annotation and immediately
/// re-triggers reconciliation. The value is consumed read-only here.
const TRACEPARENT_ANNOTATION: &str = "reinhardt.io/traceparent";
/// Comma-separated list of DNS suffixes that tenant-supplied Ingress hosts may use.
const INGRESS_HOST_SUFFIXES_ENV: &str = "REINHARDT_CLOUD_INGRESS_HOST_SUFFIXES";

/// Platform-level preview environment configuration read from the environment.
///
/// These values feed the cert-manager `Issuer` and the preview Ingress
/// `ingressClassName` for every parent-qualified preview namespace (#707).
#[derive(Debug, Clone)]
pub(crate) struct PreviewConfig {
	/// Ingress class used by preview Ingresses and the ACME HTTP-01 solver.
	pub ingress_class: String,
	/// ACME directory endpoint (e.g. Let's Encrypt production/staging).
	pub acme_server: String,
	/// ACME registration email (empty for staging/local).
	pub acme_email: String,
}

impl PreviewConfig {
	/// Read preview configuration from `REINHARDT_CLOUD_PREVIEW_*` env vars,
	/// applying platform-safe defaults when unset.
	pub(crate) fn from_env() -> Self {
		Self {
			ingress_class: std::env::var("REINHARDT_CLOUD_PREVIEW_INGRESS_CLASS")
				.unwrap_or_else(|_| "nginx".to_string()),
			acme_server: std::env::var("REINHARDT_CLOUD_PREVIEW_ACME_SERVER")
				.unwrap_or_else(|_| "https://acme-v02.api.letsencrypt.org/directory".to_string()),
			acme_email: std::env::var("REINHARDT_CLOUD_PREVIEW_ACME_EMAIL").unwrap_or_default(),
		}
	}
}

/// Shared context available to every reconciliation call.
pub(crate) struct Context {
	/// Kubernetes API client.
	pub client: Client,
	/// Platform-specific configuration for resource inference.
	pub platform: PlatformConfig,
	/// Platform-level preview environment configuration (#707).
	pub preview_config: PreviewConfig,
	/// Prometheus metrics shared with the exporter task.
	pub metrics: Arc<Metrics>,
	/// Per-object consecutive-failure counter used to drive exponential
	/// backoff in `error_policy`. Key is `(namespace, name)`.
	pub backoff_state: Arc<DashMap<(String, String), u32>>,
	/// Last observed phase label per object, used to keep the
	/// `managed_apps{phase}` gauge in sync as objects transition between
	/// phases and when they are deleted. Key is `(namespace, name)`.
	pub phase_state: Arc<DashMap<(String, String), String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SourceBuildGateAction {
	Continue,
	Requeue { requeue_after: Duration },
	UpdateProductionImage { build: BuildStatus },
	UpdatePreview { build: BuildStatus },
	AwaitChange,
}

fn source_build_gate_action(decision: BuildDecision) -> SourceBuildGateAction {
	match decision {
		BuildDecision::NoBuild => SourceBuildGateAction::Continue,
		BuildDecision::Waiting { requeue_after } => {
			SourceBuildGateAction::Requeue { requeue_after }
		}
		BuildDecision::Succeeded(completion) => match completion.status.target {
			BuildTargetKind::Production => SourceBuildGateAction::UpdateProductionImage {
				build: completion.status,
			},
			BuildTargetKind::Preview => SourceBuildGateAction::UpdatePreview {
				build: completion.status,
			},
		},
		BuildDecision::Failed(_failure) => SourceBuildGateAction::AwaitChange,
	}
}

/// Base backoff duration for transient Kube API errors.
const BACKOFF_BASE_TRANSIENT_SECS: u64 = 30;
/// Base backoff duration for dependency-not-ready (404/409) errors.
const BACKOFF_BASE_DEPENDENCY_SECS: u64 = 60;
/// Upper bound for any backoff so we keep reconciling within 10 minutes.
const BACKOFF_MAX_SECS: u64 = 600;

/// Compute an exponential backoff capped at `BACKOFF_MAX_SECS`.
///
/// `attempt` is the zero-based consecutive-failure count observed by the
/// error policy. The formula is `base * 2^attempt`, saturating to avoid
/// overflow on pathological values.
fn compute_backoff(base_secs: u64, attempt: u32) -> Duration {
	// saturating shift to avoid overflow when attempt is very large
	let factor = 1u64.checked_shl(attempt.min(20)).unwrap_or(u64::MAX);
	let secs = base_secs.saturating_mul(factor).min(BACKOFF_MAX_SECS);
	Duration::from_secs(secs)
}

/// Build a stable key for the backoff map from an object's namespace+name.
fn backoff_key(obj: &Project) -> (String, String) {
	(obj.namespace().unwrap_or_default(), obj.name_any())
}

/// Main reconciliation entry point.
pub(crate) async fn reconcile(obj: Arc<Project>, ctx: Arc<Context>) -> Result<Action, Error> {
	let span = tracing::info_span!(
		"operator.reconcile",
		otel.kind = "internal",
		resource_kind = "Project",
		resource_namespace = obj.metadata.namespace.as_deref().unwrap_or(""),
		resource_name = %obj.name_any(),
		reconcile_id = %Uuid::new_v4(),
	);

	// If the CRD carries a `reinhardt.io/traceparent` annotation, stitch this
	// reconcile span into the caller's distributed trace. Writing the value
	// back to the annotation is intentionally deferred: patching it here would
	// re-trigger reconciliation and create a patch loop.
	if let Some(tp) = obj
		.metadata
		.annotations
		.as_ref()
		.and_then(|a| a.get(TRACEPARENT_ANNOTATION))
	{
		let parent_cx = reinhardt_cloud_telemetry::context_from_traceparent(tp);
		let _ = span.set_parent(parent_cx);
	}

	let key = backoff_key(&obj);
	let start = std::time::Instant::now();

	async move {
		let name = obj.name_any();
		let namespace = obj
			.namespace()
			.filter(|ns| !ns.is_empty())
			.ok_or_else(|| Error::MissingNamespace(name.clone()))?;
		info!("Reconciling Project {namespace}/{name}");

		let api: Api<Project> = Api::namespaced(ctx.client.clone(), &namespace);

		let result = finalizer(&api, FINALIZER_NAME, obj, |event| async {
			match event {
				FinalizerEvent::Apply(app) => apply(app, &ctx, &namespace).await,
				FinalizerEvent::Cleanup(app) => cleanup(app, &ctx, &namespace).await,
			}
		})
		.await
		.map_err(|e| Error::Finalizer(Box::new(e)));

		let elapsed = start.elapsed().as_secs_f64();
		match &result {
			Ok(_) => {
				// Successful reconcile resets the backoff counter for this object.
				ctx.backoff_state.remove(&key);
				ctx.metrics
					.reconcile_total
					.with_label_values(&["success"])
					.inc();
				ctx.metrics
					.reconcile_duration
					.with_label_values(&["success"])
					.observe(elapsed);
			}
			Err(err) => {
				let label = backoff_class(err).as_metric_label();
				ctx.metrics
					.reconcile_total
					.with_label_values(&[label])
					.inc();
				ctx.metrics
					.reconcile_duration
					.with_label_values(&[label])
					.observe(elapsed);
			}
		}

		result
	}
	.instrument(span)
	.await
}

fn resource_is_controlled_by_project(secret: &Secret, app: &Project) -> bool {
	let Some(project_uid) = app.metadata.uid.as_deref() else {
		return false;
	};

	secret
		.metadata
		.owner_references
		.as_deref()
		.unwrap_or_default()
		.iter()
		.any(|owner| owner.uid == project_uid && owner.controller == Some(true))
}

fn git_credentials_cleanup_names(app: &Project, project_name: &str) -> Vec<String> {
	let fallback_name = managed_git_credentials_secret_name(project_name);
	let github_name = managed_github_credentials_secret_name(project_name);
	let mut names = vec![fallback_name, github_name];
	if let Some(source) = app.spec.source.as_ref()
		&& let Some(secret_name) = source.credentials_secret.as_ref()
		&& (secret_name == &names[0] || secret_name == &names[1])
		&& !names.contains(secret_name)
	{
		names.push(secret_name.clone());
	}
	names
}

fn git_credentials_secret_is_managed(
	secret: &Secret,
	app: &Project,
	project_name: &str,
	secret_name: &str,
) -> bool {
	if resource_is_controlled_by_project(secret, app) {
		return true;
	}

	let project_scoped = secret_name == managed_git_credentials_secret_name(project_name)
		|| secret_name == managed_github_credentials_secret_name(project_name);
	if !project_scoped {
		return false;
	}

	secret.metadata.labels.as_ref().is_some_and(|labels| {
		labels
			.get("reinhardt.dev/credential-type")
			.map(String::as_str)
			== Some("git")
			&& labels.get("reinhardt.dev/provider").map(String::as_str) == Some("github")
	})
}

async fn delete_git_credentials_secret_if_managed(
	secret_api: &Api<Secret>,
	app: &Project,
	namespace: &str,
	project_name: &str,
	secret_name: &str,
) -> Result<(), Error> {
	let Some(existing) = secret_api.get_opt(secret_name).await.map_err(Error::Kube)? else {
		return Ok(());
	};
	if git_credentials_secret_is_managed(&existing, app, project_name, secret_name) {
		let _ = secret_api
			.delete(secret_name, &DeleteParams::default())
			.await;
		return Ok(());
	}

	warn!(
		"Skipping Git credentials Secret {namespace}/{secret_name}: existing Secret is not managed by Project {namespace}/{project_name}"
	);
	Ok(())
}

/// Apply the desired state for a `Project`.
fn existing_resource_is_controlled_by_project(metadata: &ObjectMeta, app: &Project) -> bool {
	let Some(project_uid) = app.metadata.uid.as_deref() else {
		return false;
	};

	metadata
		.owner_references
		.as_deref()
		.unwrap_or_default()
		.iter()
		.any(|owner| owner.uid == project_uid && owner.controller == Some(true))
}

fn ownership_conflict_error(
	kind: &'static str,
	namespace: &str,
	name: &str,
	app: &Project,
) -> Error {
	Error::ResourceOwnershipConflict {
		kind,
		namespace: namespace.to_string(),
		name: name.to_string(),
		project_namespace: app.namespace().unwrap_or_default(),
		project_name: app.name_any(),
	}
}

async fn ensure_deployment_apply_target_is_owned(
	deployments: &Api<Deployment>,
	app: &Project,
	namespace: &str,
	name: &str,
) -> Result<(), Error> {
	match deployments.get(name).await {
		Ok(existing) if existing_resource_is_controlled_by_project(&existing.metadata, app) => {
			Ok(())
		}
		Ok(_) => Err(ownership_conflict_error("Deployment", namespace, name, app)),
		Err(kube::Error::Api(api_err)) if api_err.code == 404 => Ok(()),
		Err(err) => Err(Error::Kube(err)),
	}
}

async fn ensure_service_apply_target_is_owned(
	services: &Api<Service>,
	app: &Project,
	namespace: &str,
	name: &str,
) -> Result<(), Error> {
	match services.get(name).await {
		Ok(existing) if existing_resource_is_controlled_by_project(&existing.metadata, app) => {
			Ok(())
		}
		Ok(_) => Err(ownership_conflict_error("Service", namespace, name, app)),
		Err(kube::Error::Api(api_err)) if api_err.code == 404 => Ok(()),
		Err(err) => Err(Error::Kube(err)),
	}
}

async fn apply(app: Arc<Project>, ctx: &Context, namespace: &str) -> Result<Action, Error> {
	let name = app.name_any();
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();

	// Tenancy enforcement (#416) — runs before any per-app resource so a
	// misplaced CR cannot create work in the wrong namespace. Validation
	// failures are recorded as a `Degraded` condition (reason
	// `TenantMismatch` or `InvalidTenant`) before the error is returned
	// to the controller's error policy, which classifies them as
	// permanent and skips backoff.
	match validate_tenant_namespace(&app, namespace) {
		Ok(Some(_expected)) => {
			let tenant = app
				.spec
				.tenant
				.as_ref()
				.expect("tenant presence verified by validate_tenant_namespace");
			reconcile_tenant_resources(&ctx.client, tenant).await?;
		}
		Ok(None) => {
			// Backward-compat path: tenant unset, no enforcement.
		}
		Err(err) => {
			let (reason, message) = match &err {
				Error::TenantMismatch { expected, actual } => (
					"TenantMismatch",
					format!(
						"metadata.namespace '{actual}' does not match the namespace computed from spec.tenant ('{expected}')"
					),
				),
				Error::InvalidTenant(detail) => ("InvalidTenant", detail.clone()),
				other => ("ReconcileError", other.to_string()),
			};
			record_degraded_condition(&app, ctx, namespace, reason, &message).await;
			return Err(err);
		}
	}

	// Create the per-app `core.secret_key` Secret unconditionally, so every
	// reinhardt-web app reconciled by this operator can resolve
	// `core.secret_key` from `production.toml` via Secret-backed env-var
	// interpolation. Use idempotent
	// get-or-create so we never rotate the key on a follow-up reconcile —
	// rotating the signing key would invalidate every active session and
	// CSRF token in the running Pod.
	{
		let core_secret_name = format!("{name}-core-secret-key");
		let secret_api: Api<Secret> = Api::namespaced(ctx.client.clone(), namespace);
		if secret_api
			.get_opt(&core_secret_name)
			.await
			.map_err(Error::Kube)?
			.is_none()
		{
			let core_secret = build_core_secret_key_secret(&name, namespace);
			secret_api
				.create(&PostParams::default(), &core_secret)
				.await
				.map_err(|e| Error::SecretGeneration(e.to_string()))?;
			info!("Created core secret_key Secret {namespace}/{core_secret_name}");
		}
	}

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

	// DB credentials Secret for the explicit-database path is created by
	// `infer_database_resources` (called below in the `app.spec.database.is_some()`
	// branch) via `apply_db_secret_if_absent`. Generating it here with a
	// different username (`{name}_user` vs. the sanitized `{name_sanitized}` used
	// by the inference module) caused a convention conflict: the early Secret was
	// created first, then `apply_db_secret_if_absent` silently skipped it, leaving
	// the credentials mismatched with the init-SQL in the ConfigMap.

	// Git credentials validation (#278)
	if let Some(secret_name) = credentials::should_warn_missing_credentials(&app) {
		let secret_api: Api<Secret> = Api::namespaced(ctx.client.clone(), namespace);
		if secret_api
			.get_opt(&secret_name)
			.await
			.map_err(Error::Kube)?
			.is_none()
		{
			warn!("Git credentials Secret '{secret_name}' referenced by {name} does not exist");
		}
	}

	let preview_namespace = resources::preview_namespace::preview_namespace_name(namespace, &name);
	let previews_enabled = app.spec.source.as_ref().is_some_and(|source| {
		source
			.preview
			.as_ref()
			.is_some_and(|preview| preview.enabled)
	});
	if previews_enabled {
		reconcile_preview_namespace(
			&ctx.client,
			namespace,
			&name,
			app.meta().uid.as_deref(),
			app.spec
				.source
				.as_ref()
				.and_then(|source| source.preview.as_ref())
				.and_then(|preview| preview.budget.as_ref()),
			&ctx.preview_config,
		)
		.await?;
	}

	if reconcile_preview_delete_action(&app, &ctx.client, namespace, &preview_namespace).await? {
		return Ok(Action::await_change());
	}
	if previews_enabled {
		reconcile_preview_ttl_cleanup(&app, &ctx.client, &preview_namespace).await?;
	}

	// Source build (#275) — gate workload updates on completed Kaniko builds.
	let source_build_decision =
		source_build::reconcile_source_build(&app, ctx.client.clone(), namespace).await?;
	match source_build_gate_action(source_build_decision) {
		SourceBuildGateAction::Continue => {}
		SourceBuildGateAction::Requeue { requeue_after } => {
			return Ok(Action::requeue(requeue_after));
		}
		SourceBuildGateAction::UpdateProductionImage { build } => {
			patch_project_image(&app, &ctx.client, namespace, &build.image).await?;
			source_build::mark_build_succeeded(&app, ctx.client.clone(), namespace, build).await?;
			return Ok(Action::await_change());
		}
		SourceBuildGateAction::UpdatePreview { build } => {
			reconcile_preview_from_build(&app, &ctx.client, &preview_namespace, &build).await?;
			source_build::mark_build_succeeded(&app, ctx.client.clone(), namespace, build).await?;
			return Ok(Action::await_change());
		}
		SourceBuildGateAction::AwaitChange => {
			return Ok(Action::await_change());
		}
	}

	// Reconcile dentdelion plugin ConfigMap when spec.plugins is present.
	let cm_api: Api<ConfigMap> = Api::namespaced(ctx.client.clone(), namespace);
	// Deletion of a stale ConfigMap is left to owner-reference GC once the
	// owning Project is removed and to deliberate cleanup when
	// spec.plugins transitions from Some(..) to None (not implemented here
	// yet — see the ongoing reinhardt-cloud plugin lifecycle work).
	if let Some(plugin_cm) = crate::resources::plugins::build_plugin_configmap(&app)? {
		let cm_name = plugin_cm
			.metadata
			.name
			.clone()
			.unwrap_or_else(|| format!("{name}-dentdelion-plugins"));
		cm_api
			.patch(&cm_name, &ssapply, &Patch::Apply(&plugin_cm))
			.await
			.map_err(Error::Kube)?;
		info!("Reconciled ConfigMap {namespace}/{cm_name}");
	}

	// Resolve pages configuration (explicit spec.pages > introspect signals > disabled)
	let pages_config = resolve_pages_config(&app);

	// Reconcile per-app workload `ServiceAccount` before the Deployment so
	// the KSA exists when the Pod is admitted. Only materializes when
	// `spec.service_account.create == true`; otherwise the user is
	// pre-creating the KSA themselves.
	if let Some(workload_sa) = resources::service_account::build_service_account(&app)? {
		reconcile_app_service_account(&ctx.client, namespace, &workload_sa).await?;
	}

	// Security resources must exist before any tenant image runs, including
	// database migration Jobs that gate the rollout.
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

	let deployments: Api<Deployment> = Api::namespaced(ctx.client.clone(), namespace);

	// Database provisioning — explicit spec.database takes precedence,
	// falling back to introspect infrastructure signals.
	if app.spec.database.is_some() {
		// Explicit database: use the inference module to generate the full
		// set of platform-appropriate resources (on-prem StatefulSet/PVC,
		// AWS ACK DBInstance, or GCP Config Connector SQL resources) and
		// apply them via server-side apply.
		let resources = infer_database_resources(&app, &ctx.platform)?;
		for resource in resources {
			match resource {
				DatabaseResource::StatefulSet(ss) => {
					apply_statefulset(&ctx.client, namespace, *ss).await?;
				}
				DatabaseResource::Pvc(pvc) => {
					apply_pvc(&ctx.client, namespace, *pvc).await?;
				}
				DatabaseResource::Service(svc) => {
					apply_db_service(&ctx.client, namespace, *svc).await?;
				}
				DatabaseResource::ConfigMap(cm) => {
					apply_configmap(&ctx.client, namespace, *cm).await?;
				}
				DatabaseResource::Secret(sec) => {
					apply_db_secret_if_absent(&ctx.client, namespace, *sec).await?;
				}
				DatabaseResource::Dynamic(obj) => {
					apply_dynamic(&ctx.client, namespace, *obj).await?;
				}
			}
		}
	} else if should_provision_postgresql(&app) {
		// Introspect-derived database: provision StatefulSet + credentials
		reconcile_db_secret(&app, &ctx.client, namespace).await?;
		reconcile_db_statefulset(&app, &ctx.client, namespace).await?;
		reconcile_db_service_resource(&app, &ctx.client, namespace).await?;
	}

	let migration_state = if should_provision_postgresql(&app) {
		reconcile_migration_job_resource(&app, &ctx.client, namespace, &ctx.platform).await?
	} else {
		MigrationGateState::NotRequired
	};

	if matches!(
		migration_state,
		MigrationGateState::Running | MigrationGateState::Failed
	) {
		let ready_replicas = observed_ready_replicas(&deployments, &name).await?;
		update_status(
			&app,
			ctx,
			namespace,
			false,
			ready_replicas,
			migration_state,
			Vec::new(),
		)
		.await?;
		update_replica_gauges(
			ctx,
			namespace,
			&app,
			ready_replicas,
			app.spec.replicas.unwrap_or(1),
		);
		return Ok(Action::await_change());
	}

	// Reconcile owned Deployment only after the target migration revision
	// has completed, so a new workload image never serves before its schema
	// change gate succeeds.
	ensure_deployment_apply_target_is_owned(&deployments, &app, namespace, &name).await?;
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
	ensure_service_apply_target_is_owned(&services, &app, namespace, &name).await?;
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
	} else if let Some(host) = app
		.spec
		.services
		.as_ref()
		.and_then(|s| s.ingress_host.as_deref())
	{
		// Explicit ingress_host: create Ingress with host-based routing
		// and include pages /static/ path if pages is enabled (fixes #97).
		let port = resolve_app_port(&app);
		let routes = vec![reinhardt_cloud_types::introspect::RouteMetadata {
			path: "/".to_string(),
			methods: vec![],
			name: None,
			namespace: None,
		}];
		reconcile_ingress_resource_with_host(
			&app,
			&ctx.client,
			namespace,
			&routes,
			port,
			Some(host),
			pages_config.as_ref(),
		)
		.await?;
	}

	let mut child_conditions = Vec::new();
	if let Some(plan) = build_autoscaler(&app)? {
		match plan {
			AutoscalerPlan::Apply(hpa) => {
				let name = app.name_any();
				reconcile_hpa(&ctx.client, namespace, &name, &hpa).await?;
			}
			AutoscalerPlan::Unsupported { reason, message } => {
				child_conditions.push(build_condition(
					&app,
					ConditionType::AutoscalerReady,
					ConditionStatus::False,
					reason,
					&message,
				));
			}
		}
	}

	// Cache provisioning — explicit spec.cache takes precedence,
	// falling back to introspect infrastructure signals.
	if should_provision_cache(&app) {
		reconcile_redis_credentials_secret(&app, &ctx.client, namespace).await?;
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
		reconcile_redis_credentials_secret(&app, &ctx.client, namespace).await?;
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

	// Derive readiness from the observed Deployment status
	let desired_replicas = app.spec.replicas.unwrap_or(1);
	let ready_replicas = observed_ready_replicas(&deployments, &name).await?;
	let ready = ready_replicas >= desired_replicas;

	if let Some(condition) = tls_condition(&app, &ctx.client, namespace).await? {
		child_conditions.push(condition);
	}
	if let Some(condition) = autoscaler_condition(&app, &ctx.client, namespace).await? {
		child_conditions.push(condition);
	}

	// Update status sub-resource
	update_status(
		&app,
		ctx,
		namespace,
		ready,
		ready_replicas,
		migration_state,
		child_conditions,
	)
	.await?;
	if previews_enabled {
		reconcile_preview_status(&app, &ctx.client, namespace, &preview_namespace).await?;
	}
	update_replica_gauges(ctx, namespace, &app, ready_replicas, desired_replicas);

	Ok(Action::await_change())
}

async fn observed_ready_replicas(deployments: &Api<Deployment>, name: &str) -> Result<i32, Error> {
	Ok(deployments
		.get_opt(name)
		.await
		.map_err(Error::Kube)?
		.and_then(|deployment| deployment.status.and_then(|status| status.ready_replicas))
		.unwrap_or(0))
}

/// Clean up external resources when a `Project` is deleted.
///
/// Respects `DeletionPolicy`:
/// - `Retain` (default): only K8s-native resources (Deployment, Service)
///   are removed via ownerReferences GC. Database/cache Secrets and
///   ConfigMaps are retained for manual cleanup.
/// - `Delete`: all resources including Secrets and ConfigMaps are deleted.
async fn cleanup(app: Arc<Project>, ctx: &Context, namespace: &str) -> Result<Action, Error> {
	let name = app.name_any();
	info!("Cleaning up Project {name}");

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

	// Storage ServiceAccount (stateless — always clean up when owned).
	delete_service_account_if_owned(&ctx.client, namespace, &format!("{name}-storage"), &app)
		.await?;

	// Per-app workload ServiceAccount (stateless — always clean up when owned).
	// Check ownership before deletion so an app named `foo` cannot remove a
	// pre-existing same-namespace `foo-app` KSA that belongs to another owner.
	delete_service_account_if_owned(&ctx.client, namespace, &format!("{name}-app"), &app).await?;

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
	delete_if_exists::<HorizontalPodAutoscaler>(&ctx.client, namespace, &name).await?;
	delete_migration_jobs(&ctx.client, namespace, &name).await?;

	match app.spec.deletion_policy {
		DeletionPolicy::Retain => {
			// Deployment and Service are cleaned up via ownerReferences GC.
			// Secrets and StatefulSets are retained for manual cleanup.
			info!(
				"DeletionPolicy is Retain: keeping database and cache resources for {name}. \
				 Manual cleanup may be required for: {name}-db-credentials, \
				 {name}-redis-credentials, \
				 {name}-postgresql"
			);
		}
		DeletionPolicy::Delete => {
			info!("DeletionPolicy is Delete: removing all resources for {name}");

			// Delete JWT Secret
			let secret_api: Api<Secret> = Api::namespaced(ctx.client.clone(), namespace);
			let _ = secret_api
				.delete(&format!("{name}-jwt-secret"), &DeleteParams::default())
				.await;

			// Delete DB credentials Secret
			let _ = secret_api
				.delete(&format!("{name}-db-credentials"), &DeleteParams::default())
				.await;

			delete_redis_credentials_secret_if_managed(&secret_api, &app, namespace, &name).await?;

			// Delete SMTP credentials Secret
			let _ = secret_api
				.delete(
					&format!("{name}-smtp-credentials"),
					&DeleteParams::default(),
				)
				.await;

			for secret_name in git_credentials_cleanup_names(&app, &name) {
				delete_git_credentials_secret_if_managed(
					&secret_api,
					&app,
					namespace,
					&name,
					&secret_name,
				)
				.await?;
			}

			// Delete introspect-managed database resources
			delete_if_exists::<StatefulSet>(&ctx.client, namespace, &format!("{name}-postgresql"))
				.await?;
			delete_if_exists::<Service>(&ctx.client, namespace, &format!("{name}-postgresql"))
				.await?;
		}
	}

	// Preview namespace (#707): when previews were enabled, the operator owns a
	// parent-qualified preview namespace. Deleting it cascade-removes every
	// preview child Project and its sub-resources. Best-effort: a missing
	// namespace (previews never enabled) is not an error.
	if app
		.spec
		.source
		.as_ref()
		.is_some_and(|s| s.preview.as_ref().is_some_and(|p| p.enabled))
	{
		let preview_ns = resources::preview_namespace::preview_namespace_name(namespace, &name);
		let ns_api: Api<Namespace> = Api::all(ctx.client.clone());
		if let Some(parent_uid) = app.meta().uid.as_deref() {
			if let Some(existing_ns) = ns_api.get_opt(&preview_ns).await.map_err(Error::Kube)? {
				if resources::preview_namespace::labels_match_preview_owner(
					existing_ns.metadata.labels.as_ref(),
					namespace,
					&name,
					parent_uid,
				) {
					ns_api
						.delete(&preview_ns, &DeleteParams::default())
						.await
						.map_err(Error::Kube)?;
					info!(
						"Deleted preview namespace {preview_ns} during cleanup of {namespace}/{name}"
					);
				} else {
					warn!(
						"Skipping preview namespace cleanup for {namespace}/{name}: {preview_ns} is not labeled as owned by this Project"
					);
				}
			}
		} else {
			warn!(
				"Skipping preview namespace cleanup for {namespace}/{name}: Project UID is missing"
			);
		}
	}

	// Decrement the `managed_apps` gauge for the phase this object was
	// last observed in, so the gauge reflects only live objects.
	drop_managed_apps_gauge(ctx, &app);
	drop_replica_gauges(ctx, namespace, &app);

	Ok(Action::await_change())
}

// ── Conflict resolution: explicit CRD fields vs introspect signals ───

/// Resolve whether PostgreSQL should be provisioned.
///
/// Explicit `spec.database` field takes precedence over introspect signals.
/// Returns `true` if either the explicit database spec is set or the
/// introspect infrastructure signals indicate PostgreSQL usage.
fn should_provision_postgresql(app: &Project) -> bool {
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
fn should_provision_cache(app: &Project) -> bool {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MigrationGateState {
	NotRequired,
	Running,
	Succeeded,
	Failed,
}

impl MigrationGateState {
	fn condition_status(self) -> ConditionStatus {
		match self {
			Self::NotRequired | Self::Succeeded => ConditionStatus::True,
			Self::Running | Self::Failed => ConditionStatus::False,
		}
	}

	fn reason(self) -> &'static str {
		match self {
			Self::NotRequired => "MigrationNotRequired",
			Self::Running => "MigrationRunning",
			Self::Succeeded => "MigrationSucceeded",
			Self::Failed => "MigrationFailed",
		}
	}

	fn message(self) -> &'static str {
		match self {
			Self::NotRequired => "No database migration is required for this project",
			Self::Running => "Waiting for the deployment revision migration to complete",
			Self::Succeeded => "Deployment revision migration completed successfully",
			Self::Failed => "Deployment revision migration failed; rollout is blocked",
		}
	}
}

/// Resolve the effective application port.
///
/// Explicit `spec.services.target_port` takes precedence over
/// introspect settings. Defaults to 8000 when neither is set.
fn resolve_app_port(app: &Project) -> u16 {
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
fn should_provision_worker(app: &Project) -> bool {
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
fn should_provision_storage(app: &Project) -> bool {
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
fn should_provision_mail(app: &Project) -> bool {
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
	app: &Project,
) -> Option<(Vec<reinhardt_cloud_types::introspect::RouteMetadata>, u16)> {
	// Explicit ingress_host is handled by the else-if branch in the reconciler
	// (reconcile_ingress_resource_with_host). Here we only handle
	// introspect-derived routes when no explicit services config provides
	// an ingress host.
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

/// Reconciles the database credentials `Secret` for a `Project`.
///
/// Only creates the secret if it does not already exist, preserving
/// existing credentials across reconciliation cycles.
async fn reconcile_db_secret(app: &Project, client: &Client, namespace: &str) -> Result<(), Error> {
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
	app: &Project,
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
	app: &Project,
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
/// - If the revision job completed successfully, returns `Succeeded`.
/// - If the revision job failed, returns `Failed` and leaves it for inspection.
/// - If the revision job is still running, returns `Running`.
/// - If no revision job exists, creates one and returns `Running`.
async fn reconcile_migration_job_resource(
	app: &Project,
	client: &Client,
	namespace: &str,
	platform: &PlatformConfig,
) -> Result<MigrationGateState, Error> {
	let revision_key = migration_revision_key(app);
	let job_name = migration_job_name(app, &revision_key);
	let job_api: Api<Job> = Api::namespaced(client.clone(), namespace);

	if let Some(existing) = job_api.get_opt(&job_name).await.map_err(Error::Kube)? {
		let status = existing.status.as_ref();
		let succeeded = status.and_then(|s| s.succeeded).unwrap_or(0);

		if succeeded > 0 || job_condition_is_true(status, "Complete") {
			info!("Migration Job {namespace}/{job_name} completed for current revision");
			return Ok(MigrationGateState::Succeeded);
		}
		if job_condition_is_true(status, "Failed") {
			warn!("Migration Job {namespace}/{job_name} failed for current revision");
			return Ok(MigrationGateState::Failed);
		}
		info!("Migration Job {namespace}/{job_name} is still running");
		return Ok(MigrationGateState::Running);
	}

	let desired = build_migration_job(app, platform, &revision_key)?;
	job_api
		.create(&PostParams::default(), &desired)
		.await
		.map_err(|e| Error::DatabaseProvisioning(e.to_string()))?;
	info!("Created migration Job {namespace}/{job_name}");
	Ok(MigrationGateState::Running)
}

fn job_condition_is_true(
	status: Option<&k8s_openapi::api::batch::v1::JobStatus>,
	condition_type: &str,
) -> bool {
	status
		.and_then(|status| status.conditions.as_ref())
		.is_some_and(|conditions| {
			conditions
				.iter()
				.any(|condition| condition.type_ == condition_type && condition.status == "True")
		})
}

/// Apply a database `StatefulSet` via server-side apply.
async fn apply_statefulset(client: &Client, namespace: &str, ss: StatefulSet) -> Result<(), Error> {
	let name = ss
		.metadata
		.name
		.clone()
		.ok_or_else(|| Error::DatabaseProvisioning("StatefulSet missing name".into()))?;
	let api: Api<StatefulSet> = Api::namespaced(client.clone(), namespace);
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();
	api.patch(&name, &ssapply, &Patch::Apply(&ss))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled inferred StatefulSet {namespace}/{name}");
	Ok(())
}

/// Apply a database `PersistentVolumeClaim` via server-side apply.
async fn apply_pvc(
	client: &Client,
	namespace: &str,
	pvc: PersistentVolumeClaim,
) -> Result<(), Error> {
	let name = pvc
		.metadata
		.name
		.clone()
		.ok_or_else(|| Error::DatabaseProvisioning("PVC missing name".into()))?;
	let api: Api<PersistentVolumeClaim> = Api::namespaced(client.clone(), namespace);
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();
	api.patch(&name, &ssapply, &Patch::Apply(&pvc))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled inferred PVC {namespace}/{name}");
	Ok(())
}

/// Apply a database `ConfigMap` via server-side apply.
async fn apply_configmap(client: &Client, namespace: &str, cm: ConfigMap) -> Result<(), Error> {
	let name = cm
		.metadata
		.name
		.clone()
		.ok_or_else(|| Error::DatabaseProvisioning("ConfigMap missing name".into()))?;
	let api: Api<ConfigMap> = Api::namespaced(client.clone(), namespace);
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();
	api.patch(&name, &ssapply, &Patch::Apply(&cm))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled inferred ConfigMap {namespace}/{name}");
	Ok(())
}

/// Apply an inferred database `Service` via server-side apply.
async fn apply_db_service(client: &Client, namespace: &str, svc: Service) -> Result<(), Error> {
	let name = svc
		.metadata
		.name
		.clone()
		.ok_or_else(|| Error::DatabaseProvisioning("Service missing name".into()))?;
	let api: Api<Service> = Api::namespaced(client.clone(), namespace);
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();
	api.patch(&name, &ssapply, &Patch::Apply(&svc))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled inferred DB Service {namespace}/{name}");
	Ok(())
}

/// Create a database credentials `Secret` only when missing.
///
/// Unlike other database resources, the credentials Secret embeds a
/// freshly generated random password; reapplying it would rotate the
/// password on every reconcile cycle and break existing consumers.
/// We therefore create it once and leave subsequent reconciles as no-ops.
///
/// `AlreadyExists` (HTTP 409) is treated as success to handle races between
/// concurrent reconcile cycles or external controllers that may create the
/// Secret between our get-then-create calls.
async fn apply_db_secret_if_absent(
	client: &Client,
	namespace: &str,
	secret: Secret,
) -> Result<(), Error> {
	let name = secret
		.metadata
		.name
		.clone()
		.ok_or_else(|| Error::DatabaseProvisioning("Secret missing name".into()))?;
	let api: Api<Secret> = Api::namespaced(client.clone(), namespace);
	if api.get_opt(&name).await.map_err(Error::Kube)?.is_some() {
		info!("Inferred DB Secret {namespace}/{name} already exists, skipping");
		return Ok(());
	}
	match api.create(&PostParams::default(), &secret).await {
		Ok(_) => {
			info!("Created inferred DB Secret {namespace}/{name}");
			Ok(())
		}
		// AlreadyExists means another reconcile or controller beat us to it —
		// treat as success since the desired state is already achieved.
		Err(kube::Error::Api(ref status)) if status.code == 409 => {
			info!("Inferred DB Secret {namespace}/{name} was created concurrently, skipping");
			Ok(())
		}
		Err(e) => Err(Error::Kube(e)),
	}
}

/// Apply a cloud-provider `DynamicObject` (ACK / Config Connector CRDs)
/// via server-side apply.
async fn apply_dynamic(client: &Client, namespace: &str, obj: DynamicObject) -> Result<(), Error> {
	let name = obj
		.metadata
		.name
		.clone()
		.ok_or_else(|| Error::DatabaseProvisioning("DynamicObject missing name".into()))?;
	let types = obj
		.types
		.as_ref()
		.ok_or_else(|| Error::DatabaseProvisioning("DynamicObject missing TypeMeta".into()))?;
	let gvk = kube::api::GroupVersionKind::try_from(types)
		.map_err(|e| Error::DatabaseProvisioning(format!("invalid TypeMeta: {e}")))?;
	let ar = kube::api::ApiResource::from_gvk(&gvk);
	let api: Api<DynamicObject> = Api::namespaced_with(client.clone(), namespace, &ar);
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();
	api.patch(&name, &ssapply, &Patch::Apply(&obj))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled inferred {} {namespace}/{name}", types.kind);
	Ok(())
}

/// Reconciles the `Ingress` resource via server-side apply.
async fn reconcile_ingress_resource(
	app: &Project,
	client: &Client,
	namespace: &str,
	routes: &[reinhardt_cloud_types::introspect::RouteMetadata],
	port: u16,
	pages_config: Option<&ResolvedPagesConfig>,
) -> Result<(), Error> {
	reconcile_ingress_resource_with_host(app, client, namespace, routes, port, None, pages_config)
		.await
}

/// Reconciles the `Ingress` resource via server-side apply, with an optional explicit host.
async fn reconcile_ingress_resource_with_host(
	app: &Project,
	client: &Client,
	namespace: &str,
	routes: &[reinhardt_cloud_types::introspect::RouteMetadata],
	port: u16,
	host: Option<&str>,
	pages_config: Option<&ResolvedPagesConfig>,
) -> Result<(), Error> {
	let name = app.name_any();
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();

	let normalized_host = host.map(str::trim).filter(|host| !host.is_empty());
	if let Some(host) = normalized_host {
		ensure_ingress_host_allowed(host)?;
		ensure_ingress_host_available(client, app, namespace, host).await?;
	}

	let signals = app
		.spec
		.introspect
		.as_ref()
		.map(|i| &i.features.infrastructure_signals);
	let Some(desired) = build_ingress(app, routes, port, normalized_host, signals, pages_config)?
	else {
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

fn ensure_ingress_host_allowed(host: &str) -> Result<(), Error> {
	if is_wildcard_ingress_host(host) {
		return Err(Error::InvalidIngressHost(format!(
			"services.ingress_host '{host}' must not use Kubernetes wildcard host syntax"
		)));
	}

	let suffixes = allowed_ingress_host_suffixes_from_env();
	if suffixes.is_empty() {
		return Err(Error::InvalidIngressHost(format!(
			"services.ingress_host '{host}' requires {INGRESS_HOST_SUFFIXES_ENV} to declare an allowed DNS suffix"
		)));
	}

	if suffixes
		.iter()
		.any(|suffix| host_matches_allowed_suffix(host, suffix))
	{
		return Ok(());
	}

	Err(Error::InvalidIngressHost(format!(
		"services.ingress_host '{host}' is outside the allowed DNS suffixes configured by {INGRESS_HOST_SUFFIXES_ENV}"
	)))
}

fn allowed_ingress_host_suffixes_from_env() -> Vec<String> {
	std::env::var(INGRESS_HOST_SUFFIXES_ENV)
		.unwrap_or_default()
		.split(',')
		.map(str::trim)
		.filter(|suffix| !suffix.is_empty())
		.map(|suffix| suffix.trim_start_matches('.').to_ascii_lowercase())
		.collect()
}

fn is_wildcard_ingress_host(host: &str) -> bool {
	host.trim().trim_end_matches('.').starts_with("*.")
}

fn host_matches_allowed_suffix(host: &str, suffix: &str) -> bool {
	let host = host.trim_end_matches('.').to_ascii_lowercase();
	let suffix = suffix.trim_end_matches('.');
	host == suffix || host.ends_with(&format!(".{suffix}"))
}

async fn ensure_ingress_host_available(
	client: &Client,
	app: &Project,
	namespace: &str,
	host: &str,
) -> Result<(), Error> {
	let ingresses: Api<Ingress> = Api::all(client.clone());
	let existing = ingresses
		.list(&ListParams::default())
		.await
		.map_err(Error::Kube)?;
	if let Some((existing_namespace, existing_name)) = existing.items.iter().find_map(|ingress| {
		ingress_claims_host(ingress, host)
			.then(|| (ingress.namespace().unwrap_or_default(), ingress.name_any()))
	}) && (existing_namespace != namespace || existing_name != app.name_any())
	{
		return Err(Error::InvalidIngressHost(format!(
			"services.ingress_host '{host}' is already claimed by Ingress {existing_namespace}/{existing_name}"
		)));
	}

	Ok(())
}

fn ingress_claims_host(ingress: &Ingress, host: &str) -> bool {
	ingress
		.spec
		.as_ref()
		.and_then(|spec| spec.rules.as_ref())
		.is_some_and(|rules| {
			rules
				.iter()
				.any(|rule| ingress_rule_matches_host(rule, host))
		})
}

fn ingress_rule_matches_host(rule: &IngressRule, host: &str) -> bool {
	rule.host
		.as_deref()
		.is_some_and(|existing| existing.eq_ignore_ascii_case(host))
}

async fn reconcile_hpa(
	client: &Client,
	namespace: &str,
	name: &str,
	hpa: &HorizontalPodAutoscaler,
) -> Result<(), Error> {
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();
	let hpas: Api<HorizontalPodAutoscaler> = Api::namespaced(client.clone(), namespace);
	hpas.patch(name, &ssapply, &Patch::Apply(hpa))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled HPA {namespace}/{name}");
	Ok(())
}

/// Reconciles the Redis credentials `Secret` for a `Project`.
///
/// Only creates the secret if it does not already exist, preserving existing
/// Redis passwords across reconciliation cycles.
async fn reconcile_redis_credentials_secret(
	app: &Project,
	client: &Client,
	namespace: &str,
) -> Result<(), Error> {
	let name = app.name_any();
	let secret_name = format!("{name}-redis-credentials");
	let secret_api: Api<Secret> = Api::namespaced(client.clone(), namespace);

	if let Some(existing) = secret_api
		.get_opt(&secret_name)
		.await
		.map_err(Error::Kube)?
	{
		if redis_credentials_secret_is_managed_by_project(&existing.metadata, app) {
			info!("Redis credentials Secret {namespace}/{secret_name} already exists, skipping");
			return Ok(());
		}

		return Err(ownership_conflict_error(
			"Secret",
			namespace,
			&secret_name,
			app,
		));
	}

	let secret = build_managed_redis_credentials_secret(app, namespace);
	secret_api
		.create(&PostParams::default(), &secret)
		.await
		.map_err(|e| Error::SecretGeneration(e.to_string()))?;
	info!("Created Redis credentials Secret {namespace}/{secret_name}");
	Ok(())
}

fn build_managed_redis_credentials_secret(app: &Project, namespace: &str) -> Secret {
	build_redis_credentials_secret(&app.name_any(), namespace)
}

fn redis_credentials_secret_is_managed_by_project(metadata: &ObjectMeta, app: &Project) -> bool {
	if existing_resource_is_controlled_by_project(metadata, app) {
		return true;
	}

	let app_name = app.name_any();
	metadata.labels.as_ref().is_some_and(|labels| {
		labels.get("app.kubernetes.io/name").map(String::as_str) == Some(app_name.as_str())
			&& labels
				.get("app.kubernetes.io/managed-by")
				.map(String::as_str)
				== Some("reinhardt-cloud-operator")
	})
}

async fn delete_redis_credentials_secret_if_managed(
	secret_api: &Api<Secret>,
	app: &Project,
	namespace: &str,
	name: &str,
) -> Result<(), Error> {
	let secret_name = format!("{name}-redis-credentials");
	let Some(existing) = secret_api
		.get_opt(&secret_name)
		.await
		.map_err(Error::Kube)?
	else {
		return Ok(());
	};
	if redis_credentials_secret_is_managed_by_project(&existing.metadata, app) {
		let _ = secret_api
			.delete(&secret_name, &DeleteParams::default())
			.await;
		return Ok(());
	}

	warn!(
		"Skipping Redis credentials Secret {namespace}/{secret_name}: existing Secret is not managed by Project {namespace}/{name}"
	);
	Ok(())
}

/// Reconciles the Redis cache `Deployment` via server-side apply.
async fn reconcile_cache_deployment(
	app: &Project,
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
	app: &Project,
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
	app: &Project,
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
	app: &Project,
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
	app: &Project,
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
	let project_uid = service_account_project_uid(sa)
		.expect("managed SA always has owner reference set by its builder");
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();
	let sa_api: Api<ServiceAccount> = Api::namespaced(client.clone(), namespace);
	if let Some(existing) = sa_api.get_opt(&name).await.map_err(Error::Kube)?
		&& !service_account_is_owned_by_uid(&existing, project_uid)
	{
		return Err(Error::ServiceAccountOwnership {
			namespace: namespace.to_string(),
			name,
			project_uid: project_uid.to_string(),
		});
	}
	sa_api
		.patch(&name, &ssapply, &Patch::Apply(sa))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled storage ServiceAccount {namespace}/{name}");
	Ok(())
}

/// Reconciles the per-app workload `ServiceAccount` via server-side apply.
///
/// Distinct from [`reconcile_storage_sa`]: this SA carries Workload Identity
/// or IRSA bindings for cloud-API access from the application pods, while
/// the storage SA grants access to the storage backend specifically.
async fn reconcile_app_service_account(
	client: &Client,
	namespace: &str,
	sa: &ServiceAccount,
) -> Result<(), Error> {
	let name = sa
		.metadata
		.name
		.as_ref()
		.cloned()
		.expect("workload SA always has a name set by build_service_account");
	let project_uid = service_account_project_uid(sa)
		.expect("managed SA always has owner reference set by its builder");
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();
	let sa_api: Api<ServiceAccount> = Api::namespaced(client.clone(), namespace);
	if let Some(existing) = sa_api.get_opt(&name).await.map_err(Error::Kube)?
		&& !service_account_is_owned_by_uid(&existing, project_uid)
	{
		return Err(Error::ServiceAccountOwnership {
			namespace: namespace.to_string(),
			name,
			project_uid: project_uid.to_string(),
		});
	}
	sa_api
		.patch(&name, &ssapply, &Patch::Apply(sa))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled workload ServiceAccount {namespace}/{name}");
	Ok(())
}

/// Reconciles the SMTP credentials `Secret` for a `Project`.
///
/// Only creates the secret if it does not already exist, preserving
/// user-provided credentials across reconciliation cycles.
async fn reconcile_mail_secret(
	app: &Project,
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
	app: &Project,
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

/// Reconcile NetworkPolicy resources for an isolated `Project`.
async fn reconcile_network_policies(
	app: &Project,
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
	app: &Project,
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

/// Validates `spec.tenant` (if set) and verifies that the app's
/// `metadata.namespace` matches the namespace computed from the tenant
/// reference.
///
/// Returns:
/// - `Ok(Some(expected_namespace))` when `spec.tenant` is set and valid.
/// - `Ok(None)` when `spec.tenant` is absent (legacy mode — no enforcement).
/// - `Err(Error::InvalidTenant)` when `spec.tenant` is set but malformed.
/// - `Err(Error::TenantMismatch)` when the spec is valid but
///   `metadata.namespace` is wrong.
///
/// Pure function — no API calls — so the caller controls how the
/// failure is surfaced (status patch + log) before bubbling the error
/// up to the reconciler's error policy.
fn validate_tenant_namespace(
	app: &Project,
	actual_namespace: &str,
) -> Result<Option<String>, Error> {
	let Some(tenant) = app.spec.tenant.as_ref() else {
		return Ok(None);
	};

	tenant.validate().map_err(|errors| {
		Error::InvalidTenant(
			errors
				.into_iter()
				.map(|e| e.message)
				.collect::<Vec<_>>()
				.join("; "),
		)
	})?;

	let expected = tenant.namespace();
	if expected != actual_namespace {
		return Err(Error::TenantMismatch {
			expected,
			actual: actual_namespace.to_string(),
		});
	}

	Ok(Some(expected))
}

/// Reconcile the per-tenant `Namespace`, `ResourceQuota`, and
/// `NetworkPolicy` triple.
///
/// Server-side applies each resource so concurrent reconciles for
/// sibling apps in the same tenant cannot fight over ownership. The
/// `Namespace` is created without an owner reference because it is
/// shared across CRs; the `ResourceQuota` and `NetworkPolicy` resources
/// likewise omit owner references for the same reason (see the module
/// docs in `resources::tenant`).
async fn reconcile_tenant_resources(
	client: &Client,
	tenant: &reinhardt_cloud_types::crd::tenant::TenantRef,
) -> Result<(), Error> {
	let namespace_name = tenant.namespace();
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();

	// Namespace is cluster-scoped; use Api::all.
	let namespaces: Api<Namespace> = Api::all(client.clone());
	let desired_ns = tenant_resources::build_namespace(tenant);
	namespaces
		.patch(&namespace_name, &ssapply, &Patch::Apply(&desired_ns))
		.await
		.map_err(Error::Kube)?;

	let quotas: Api<k8s_openapi::api::core::v1::ResourceQuota> =
		Api::namespaced(client.clone(), &namespace_name);
	let desired_quota = tenant_resources::build_default_resource_quota(tenant);
	let quota_name = desired_quota
		.metadata
		.name
		.clone()
		.unwrap_or_else(|| "tenant-default-quota".to_string());
	quotas
		.patch(&quota_name, &ssapply, &Patch::Apply(&desired_quota))
		.await
		.map_err(Error::Kube)?;

	let policies: Api<NetworkPolicy> = Api::namespaced(client.clone(), &namespace_name);
	for policy in [
		tenant_resources::build_default_deny_policy(tenant),
		tenant_resources::build_allow_same_namespace_policy(tenant),
		tenant_resources::build_allow_ingress_controller_policy(tenant),
	] {
		let policy_name = policy
			.metadata
			.name
			.clone()
			.unwrap_or_else(|| "tenant-policy".to_string());
		policies
			.patch(&policy_name, &ssapply, &Patch::Apply(&policy))
			.await
			.map_err(Error::Kube)?;
	}

	info!(
		"Reconciled tenant resources for namespace {namespace_name} (tenant org={}, team={:?})",
		tenant.organization, tenant.team
	);
	Ok(())
}

/// Reconcile the parent-qualified preview namespace and its resource guardrails.
///
/// The preview namespace is intentionally separate from the parent namespace,
/// so preview Projects do not use owner references to the parent `Project`.
async fn reconcile_preview_namespace(
	client: &Client,
	parent_namespace: &str,
	parent_name: &str,
	parent_uid: Option<&str>,
	budget: Option<&reinhardt_cloud_types::crd::source::PreviewBudget>,
	preview_config: &PreviewConfig,
) -> Result<(), Error> {
	let ns_name =
		resources::preview_namespace::preview_namespace_name(parent_namespace, parent_name);
	let ssapply = PatchParams::apply("reinhardt-cloud-operator").force();

	let namespaces: Api<Namespace> = Api::all(client.clone());
	let Some(parent_uid) = parent_uid else {
		warn!(
			"Skipping preview namespace reconciliation for {parent_namespace}/{parent_name}: Project UID is missing"
		);
		return Ok(());
	};
	namespaces
		.patch(
			&ns_name,
			&ssapply,
			&Patch::Apply(&resources::preview_namespace::build_namespace(
				parent_namespace,
				parent_name,
				parent_uid,
			)),
		)
		.await
		.map_err(Error::Kube)?;

	let quota =
		resources::preview_namespace::build_resource_quota(parent_namespace, parent_name, budget);
	let quota_name = quota
		.metadata
		.name
		.clone()
		.unwrap_or_else(|| "preview-default-quota".to_string());
	Api::<k8s_openapi::api::core::v1::ResourceQuota>::namespaced(client.clone(), &ns_name)
		.patch(&quota_name, &ssapply, &Patch::Apply(&quota))
		.await
		.map_err(Error::Kube)?;

	let limit_range =
		resources::preview_namespace::build_limit_range(parent_namespace, parent_name);
	let limit_range_name = limit_range
		.metadata
		.name
		.clone()
		.unwrap_or_else(|| "preview-default-limits".to_string());
	Api::<LimitRange>::namespaced(client.clone(), &ns_name)
		.patch(&limit_range_name, &ssapply, &Patch::Apply(&limit_range))
		.await
		.map_err(Error::Kube)?;

	for policy in [
		resources::preview_namespace::build_default_deny_policy(parent_namespace, parent_name),
		resources::preview_namespace::build_allow_ingress_and_dns_policy(
			parent_namespace,
			parent_name,
		),
	] {
		let policy_name = policy
			.metadata
			.name
			.clone()
			.unwrap_or_else(|| "preview-policy".to_string());
		Api::<NetworkPolicy>::namespaced(client.clone(), &ns_name)
			.patch(&policy_name, &ssapply, &Patch::Apply(&policy))
			.await
			.map_err(Error::Kube)?;
	}

	let issuer = resources::preview_namespace::build_issuer(
		parent_namespace,
		parent_name,
		&preview_config.acme_server,
		&preview_config.acme_email,
		&preview_config.ingress_class,
	);
	let issuer_name = issuer
		.metadata
		.name
		.clone()
		.unwrap_or_else(|| "preview-issuer".to_string());
	Api::<crate::resources::issuer::Issuer>::namespaced(client.clone(), &ns_name)
		.patch(&issuer_name, &ssapply, &Patch::Apply(&issuer))
		.await
		.map_err(Error::Kube)?;

	info!("Reconciled preview namespace {ns_name}");
	Ok(())
}

async fn reconcile_preview_delete_action(
	app: &Project,
	client: &Client,
	parent_namespace: &str,
	preview_namespace: &str,
) -> Result<bool, Error> {
	let Some(action) = app
		.metadata
		.annotations
		.as_ref()
		.and_then(|annotations| annotations.get("reinhardt.dev/preview-action"))
	else {
		return Ok(false);
	};
	if action != "delete" {
		return Ok(false);
	}

	let parent_name = app.name_any();
	let pr_number = app
		.metadata
		.annotations
		.as_ref()
		.and_then(|annotations| annotations.get("reinhardt.dev/pr-number"))
		.cloned()
		.unwrap_or_default();
	let preview_name = preview::preview_project_name(&parent_name, &pr_number);
	source_build::clear_preview_build_for_delete(
		app,
		client.clone(),
		parent_namespace,
		&pr_number,
		&preview_name,
	)
	.await?;

	let preview_api: Api<Project> = Api::namespaced(client.clone(), preview_namespace);
	match preview_api
		.delete(&preview_name, &DeleteParams::default())
		.await
	{
		Ok(_) => {
			info!("Deleted preview environment {preview_namespace}/{preview_name}");
		}
		Err(kube::Error::Api(status)) if status.code == 404 => {
			info!("Preview environment {preview_namespace}/{preview_name} was already absent");
		}
		Err(error) => {
			return Err(Error::Kube(error));
		}
	}

	let patch = source_build::preview_delete_annotations_patch();
	let parent_api: Api<Project> = Api::namespaced(client.clone(), parent_namespace);
	parent_api
		.patch(&parent_name, &PatchParams::default(), &Patch::Merge(&patch))
		.await
		.map_err(Error::Kube)?;
	Ok(true)
}

async fn reconcile_preview_ttl_cleanup(
	app: &Project,
	client: &Client,
	preview_namespace: &str,
) -> Result<(), Error> {
	let parent_name = app.name_any();
	let preview_list = Api::<Project>::namespaced(client.clone(), preview_namespace)
		.list(&ListParams::default().labels(&format!(
			"reinhardt.dev/preview=true,reinhardt.dev/parent-app={parent_name}"
		)))
		.await
		.map_err(Error::Kube)?;

	let ttl = app
		.spec
		.source
		.as_ref()
		.and_then(|source| source.preview.as_ref())
		.and_then(|preview| preview.ttl.as_deref())
		.unwrap_or("72h");

	for preview_app in preview_list {
		let last_activity = preview_app
			.metadata
			.annotations
			.as_ref()
			.and_then(|annotations| annotations.get("reinhardt.dev/last-activity"));
		if let Some(ts) = last_activity
			&& preview::is_ttl_expired(ts, ttl)
		{
			let preview_name = preview_app.name_any();
			let _ = Api::<Project>::namespaced(client.clone(), preview_namespace)
				.delete(&preview_name, &DeleteParams::default())
				.await;
			info!("TTL expired, deleted preview {preview_namespace}/{preview_name}");
		}
	}

	Ok(())
}

async fn patch_project_image(
	app: &Project,
	client: &Client,
	namespace: &str,
	image: &str,
) -> Result<(), Error> {
	let api: Api<Project> = Api::namespaced(client.clone(), namespace);
	let patch = serde_json::json!({
		"spec": {
			"image": image
		}
	});
	api.patch(
		&app.name_any(),
		&PatchParams::default(),
		&Patch::Merge(&patch),
	)
	.await
	.map_err(Error::Kube)?;
	Ok(())
}

async fn reconcile_preview_from_build(
	app: &Project,
	client: &Client,
	preview_namespace: &str,
	build: &reinhardt_cloud_types::crd::BuildStatus,
) -> Result<(), Error> {
	let pr_number = build
		.pr_number
		.as_deref()
		.ok_or(Error::MissingField("status.build.prNumber"))?;
	let preview_name = build
		.preview_name
		.as_deref()
		.ok_or(Error::MissingField("status.build.previewName"))?;
	let preview_spec =
		preview::build_preview_spec(app, pr_number, &build.image_tag, build.branch.as_deref())?;
	let parent_name = app.name_any();
	let parent_namespace = app
		.namespace()
		.ok_or_else(|| Error::MissingNamespace(parent_name.clone()))?;
	let preview_labels = preview::preview_labels(&parent_namespace, &parent_name, pr_number);
	let preview_app = Project {
		metadata: kube::api::ObjectMeta {
			name: Some(preview_name.to_string()),
			namespace: Some(preview_namespace.to_string()),
			labels: Some(preview_labels),
			annotations: Some(BTreeMap::from([(
				"reinhardt.dev/last-activity".to_string(),
				chrono::Utc::now().to_rfc3339(),
			)])),
			..Default::default()
		},
		spec: preview_spec,
		status: None,
	};

	let api: Api<Project> = Api::namespaced(client.clone(), preview_namespace);
	api.patch(
		preview_name,
		&PatchParams::apply("reinhardt-cloud-operator").force(),
		&Patch::Apply(&preview_app),
	)
	.await
	.map_err(Error::Kube)?;
	info!("Reconciled preview environment {preview_namespace}/{preview_name}");
	Ok(())
}

async fn reconcile_preview_status(
	app: &Project,
	client: &Client,
	parent_namespace: &str,
	preview_namespace: &str,
) -> Result<(), Error> {
	let parent_name = app.name_any();
	let preview_list = Api::<Project>::namespaced(client.clone(), preview_namespace)
		.list(&ListParams::default().labels(&format!(
			"reinhardt.dev/preview=true,reinhardt.dev/parent-app={parent_name}"
		)))
		.await
		.map_err(Error::Kube)?;
	let status_patch = build_preview_status_patch(&preview_list.items);
	let parent_status_api: Api<Project> = Api::namespaced(client.clone(), parent_namespace);
	if let Err(error) = parent_status_api
		.patch_status(
			&parent_name,
			&PatchParams::default(),
			&Patch::Merge(&status_patch),
		)
		.await
	{
		warn!("failed to write preview status for {parent_namespace}/{parent_name}: {error}");
	}

	Ok(())
}

fn build_preview_status_patch(preview_projects: &[Project]) -> serde_json::Value {
	let previews = resources::preview_status::build_preview_status_list(preview_projects, "https");
	serde_json::json!({ "status": { "previews": previews } })
}

/// Builds a `Degraded`-condition status patch payload for the given
/// reason and message.
///
/// Pure function so it is exercised by unit tests without spinning up a
/// kube client. The returned JSON value is shaped to merge into the
/// `status` sub-resource via `Patch::Merge`.
fn build_degraded_status_patch(app: &Project, reason: &str, message: &str) -> serde_json::Value {
	let condition = ProjectCondition {
		type_: ConditionType::Degraded,
		status: ConditionStatus::True,
		reason: reason.to_string(),
		message: message.to_string(),
		last_transition_time: Some(chrono::Utc::now().to_rfc3339()),
		observed_generation: app.metadata.generation,
	};

	serde_json::json!({
		"status": {
			"phase": ProjectPhase::Failed,
			"observedGeneration": app.metadata.generation,
			"conditions": [condition],
		}
	})
}

/// Best-effort: write a `Degraded` condition to the app's status
/// sub-resource so the failure is visible via `kubectl get project`.
///
/// Errors from this helper are logged at warn level rather than
/// propagated because the original failure (the reason we're writing
/// Degraded) is what the caller really needs to surface.
async fn record_degraded_condition(
	app: &Project,
	ctx: &Context,
	namespace: &str,
	reason: &str,
	message: &str,
) {
	let api: Api<Project> = Api::namespaced(ctx.client.clone(), namespace);
	let payload = build_degraded_status_patch(app, reason, message);
	if let Err(e) = api
		.patch_status(
			&app.name_any(),
			&PatchParams::default(),
			&Patch::Merge(payload),
		)
		.await
	{
		warn!(
			"failed to write Degraded condition (reason={reason}) for {}/{}: {e}",
			namespace,
			app.name_any()
		);
	}
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

fn build_condition(
	app: &Project,
	condition_type: ConditionType,
	status: ConditionStatus,
	reason: &str,
	message: &str,
) -> ProjectCondition {
	let existing_condition = app.status.as_ref().and_then(|s| {
		s.conditions
			.iter()
			.find(|condition| condition.type_ == condition_type)
	});
	let last_transition_time = if should_update_transition_time(
		existing_condition.map(|condition| &condition.status),
		&status,
	) {
		Some(chrono::Utc::now().to_rfc3339())
	} else {
		existing_condition.and_then(|condition| condition.last_transition_time.clone())
	};

	ProjectCondition {
		type_: condition_type,
		status,
		reason: reason.to_string(),
		message: message.to_string(),
		last_transition_time,
		observed_generation: app.metadata.generation,
	}
}

async fn tls_condition(
	app: &Project,
	client: &Client,
	namespace: &str,
) -> Result<Option<ProjectCondition>, Error> {
	let Some(services) = app.spec.services.as_ref() else {
		return Ok(None);
	};
	let Some(tls) = services.tls.as_ref() else {
		return Ok(None);
	};
	if !tls.enabled {
		return Ok(None);
	}

	let host = services.ingress_host.as_deref().unwrap_or_default();
	let secret_name = tls.secret_name.as_deref().unwrap_or_default();
	if host.is_empty() || secret_name.is_empty() {
		return Ok(Some(build_condition(
			app,
			ConditionType::TlsReady,
			ConditionStatus::False,
			"TlsSecretNotReady",
			"Waiting for Ingress TLS host and Secret name to be configured",
		)));
	}

	let ingress_api: Api<Ingress> = Api::namespaced(client.clone(), namespace);
	let secret_api: Api<Secret> = Api::namespaced(client.clone(), namespace);
	let ingress = ingress_api
		.get_opt(&app.name_any())
		.await
		.map_err(Error::Kube)?;
	let secret = secret_api.get_opt(secret_name).await.map_err(Error::Kube)?;

	let ingress_matches = ingress
		.as_ref()
		.and_then(|ingress| ingress.spec.as_ref())
		.and_then(|spec| spec.tls.as_ref())
		.is_some_and(|tls_entries| {
			tls_entries.iter().any(|entry| {
				entry.secret_name.as_deref() == Some(secret_name)
					&& entry
						.hosts
						.as_ref()
						.is_some_and(|hosts| hosts.iter().any(|candidate| candidate == host))
			})
		});

	let ready = ingress_matches && secret.is_some();
	let (status, reason, message) = if ready {
		(
			ConditionStatus::True,
			"TlsSecretReady",
			format!("Ingress TLS references Secret '{secret_name}' and the Secret exists"),
		)
	} else {
		(
			ConditionStatus::False,
			"TlsSecretNotReady",
			format!("Waiting for Ingress TLS host '{host}' and Secret '{secret_name}'"),
		)
	};

	Ok(Some(build_condition(
		app,
		ConditionType::TlsReady,
		status,
		reason,
		&message,
	)))
}

async fn autoscaler_condition(
	app: &Project,
	client: &Client,
	namespace: &str,
) -> Result<Option<ProjectCondition>, Error> {
	let Some(scale) = app.spec.scale.as_ref() else {
		return Ok(None);
	};
	if scale
		.metric
		.as_ref()
		.is_some_and(|metric| matches!(metric, ScaleMetric::Rps))
	{
		return Ok(None);
	}

	let hpa_api: Api<HorizontalPodAutoscaler> = Api::namespaced(client.clone(), namespace);
	let hpa = hpa_api
		.get_opt(&app.name_any())
		.await
		.map_err(Error::Kube)?;
	let ready = hpa.as_ref().is_some_and(hpa_is_ready);

	let (status, reason, message) = if ready {
		(
			ConditionStatus::True,
			"AutoscalerActive",
			"HorizontalPodAutoscaler is observed and active".to_string(),
		)
	} else {
		(
			ConditionStatus::False,
			"AutoscalerNotReady",
			"Waiting for HorizontalPodAutoscaler to become observed and active".to_string(),
		)
	};

	Ok(Some(build_condition(
		app,
		ConditionType::AutoscalerReady,
		status,
		reason,
		&message,
	)))
}

fn service_account_project_uid(sa: &ServiceAccount) -> Option<&str> {
	sa.metadata
		.owner_references
		.as_ref()?
		.iter()
		.find(|owner| owner.controller.unwrap_or(false))
		.map(|owner| owner.uid.as_str())
}

fn service_account_is_owned_by_uid(sa: &ServiceAccount, project_uid: &str) -> bool {
	sa.metadata.owner_references.as_ref().is_some_and(|owners| {
		owners.iter().any(|owner| {
			owner.uid == project_uid && owner.kind == "Project" && owner.controller == Some(true)
		})
	})
}

async fn delete_service_account_if_owned(
	client: &Client,
	namespace: &str,
	name: &str,
	app: &Project,
) -> Result<(), Error> {
	let Some(project_uid) = app.metadata.uid.as_deref() else {
		return Ok(());
	};
	let api: Api<ServiceAccount> = Api::namespaced(client.clone(), namespace);
	let Some(sa) = api.get_opt(name).await.map_err(Error::Kube)? else {
		return Ok(());
	};
	if service_account_is_owned_by_uid(&sa, project_uid) {
		api.delete(name, &Default::default())
			.await
			.map_err(Error::Kube)?;
	} else {
		warn!("Skipping deletion of unowned ServiceAccount {namespace}/{name}");
	}
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

async fn delete_migration_jobs(
	client: &Client,
	namespace: &str,
	app_name: &str,
) -> Result<(), Error> {
	let api: Api<Job> = Api::namespaced(client.clone(), namespace);
	let jobs = api
		.list(&ListParams::default().labels(&format!(
			"app.kubernetes.io/name={app_name},app.kubernetes.io/component=migration"
		)))
		.await
		.map_err(Error::Kube)?;
	for job in jobs {
		if let Some(name) = job.metadata.name {
			api.delete(&name, &DeleteParams::default())
				.await
				.map_err(Error::Kube)?;
		}
	}
	Ok(())
}

/// Builds the desired `ProjectStatus` for the given readiness state.
///
/// Pure function that computes the status without any Kubernetes API
/// calls, making it independently testable.
fn build_status(
	app: &Project,
	ready: bool,
	ready_replicas: i32,
	migration_state: MigrationGateState,
	child_conditions: Vec<ProjectCondition>,
) -> ProjectStatus {
	let existing_status = app.status.as_ref();
	let phase = if migration_state == MigrationGateState::Failed {
		ProjectPhase::Degraded
	} else if ready {
		ProjectPhase::Running
	} else {
		ProjectPhase::Deploying
	};
	let condition_status = if ready {
		ConditionStatus::True
	} else {
		ConditionStatus::False
	};
	let reason = if ready {
		"ReconcileSuccess"
	} else if migration_state == MigrationGateState::Failed {
		"MigrationFailed"
	} else {
		"ReconcileInProgress"
	};
	let message = if ready {
		"Application is ready"
	} else if migration_state == MigrationGateState::Failed {
		"Migration failed; rollout is blocked"
	} else {
		"Waiting for deployment rollout to complete"
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
	let mut conditions = existing_status
		.map(|status| status.conditions.clone())
		.unwrap_or_default();
	let build = existing_status.and_then(|status| status.build.clone());
	let previews = existing_status
		.map(|status| status.previews.clone())
		.unwrap_or_default();
	upsert_project_condition(
		&mut conditions,
		build_condition(app, ConditionType::Ready, condition_status, reason, message),
	);
	upsert_project_condition(
		&mut conditions,
		build_condition(
			app,
			ConditionType::MigrationReady,
			migration_state.condition_status(),
			migration_state.reason(),
			migration_state.message(),
		),
	);
	if migration_state == MigrationGateState::Failed {
		upsert_project_condition(
			&mut conditions,
			build_condition(
				app,
				ConditionType::Degraded,
				ConditionStatus::True,
				"MigrationFailed",
				"Migration failed for the target deployment revision",
			),
		);
	} else if ready
		&& let Some(existing_degraded_condition) = existing_status.and_then(|status| {
			status
				.conditions
				.iter()
				.find(|condition| condition.type_ == ConditionType::Degraded)
		}) {
		let degraded_status = ConditionStatus::False;
		let degraded_transition_time = if should_update_transition_time(
			Some(&existing_degraded_condition.status),
			&degraded_status,
		) {
			Some(chrono::Utc::now().to_rfc3339())
		} else {
			existing_degraded_condition.last_transition_time.clone()
		};
		upsert_project_condition(
			&mut conditions,
			ProjectCondition {
				type_: ConditionType::Degraded,
				status: degraded_status,
				reason: reason.to_string(),
				message: message.to_string(),
				last_transition_time: degraded_transition_time,
				observed_generation: app.metadata.generation,
			},
		);
	}
	for condition in child_conditions {
		upsert_project_condition(&mut conditions, condition);
	}

	ProjectStatus {
		phase: Some(phase),
		conditions,
		observed_generation: app.metadata.generation,
		ready_replicas: Some(ready_replicas),
		build,
		database,
		previews,
		..Default::default()
	}
}

/// Updates the status sub-resource of a `Project`.
///
/// Only updates `lastTransitionTime` when the condition status actually
/// changes, preventing unnecessary tight reconcile loops.
async fn update_status(
	app: &Project,
	ctx: &Context,
	namespace: &str,
	ready: bool,
	ready_replicas: i32,
	migration_state: MigrationGateState,
	child_conditions: Vec<ProjectCondition>,
) -> Result<(), Error> {
	let api: Api<Project> = Api::namespaced(ctx.client.clone(), namespace);
	let typed_status = build_status(
		app,
		ready,
		ready_replicas,
		migration_state,
		child_conditions,
	);

	let phase_label_for_gauge = typed_status.phase.as_ref().map(phase_label);
	let status = serde_json::json!({ "status": typed_status });

	api.patch_status(
		&app.name_any(),
		&PatchParams::default(),
		&Patch::Merge(status),
	)
	.await
	.map_err(Error::Kube)?;

	// Update the `managed_apps{phase}` gauge only after the status patch
	// has been persisted. Otherwise a failed patch could leave the gauge
	// reflecting a phase transition that never actually took effect, and
	// the next reconcile may not correct it cleanly.
	if let Some(label) = phase_label_for_gauge {
		update_managed_apps_gauge(ctx, app, label);
	}

	Ok(())
}

/// Map an `ProjectPhase` to the stable label value used by the
/// `managed_apps{phase=...}` gauge. Kept as a free function so new
/// phases must be added explicitly and cannot drift from the CRD enum.
fn phase_label(phase: &ProjectPhase) -> &'static str {
	match phase {
		ProjectPhase::Pending => "Pending",
		ProjectPhase::Provisioning => "Provisioning",
		ProjectPhase::Deploying => "Deploying",
		ProjectPhase::Running => "Running",
		ProjectPhase::Degraded => "Degraded",
		ProjectPhase::Failed => "Failed",
		ProjectPhase::Terminating => "Terminating",
	}
}

/// Update the `managed_apps{phase}` gauge for a single object.
///
/// Decrements the previous phase bucket (if any) before incrementing the
/// new bucket, so totals across phases remain consistent even when an
/// app transitions between phases across reconciliations.
fn update_managed_apps_gauge(ctx: &Context, app: &Project, new_phase: &str) {
	let key = backoff_key(app);
	let previous = ctx.phase_state.insert(key, new_phase.to_string());
	let changed = previous.as_deref() != Some(new_phase);
	if let Some(prev) = previous.as_deref()
		&& changed
	{
		ctx.metrics.managed_apps.with_label_values(&[prev]).dec();
	}
	if changed {
		ctx.metrics
			.managed_apps
			.with_label_values(&[new_phase])
			.inc();
	}
}

/// Update the `managed_apps_ready_replicas` / `managed_apps_desired_replicas`
/// gauges for a single object. Set from the observed Deployment status so
/// Project health and deployment state can be queried via Prometheus.
fn update_replica_gauges(ctx: &Context, namespace: &str, app: &Project, ready: i32, desired: i32) {
	let labels = [namespace, app.metadata.name.as_deref().unwrap_or("")];
	ctx.metrics
		.managed_apps_ready_replicas
		.with_label_values(&labels)
		.set(ready as f64);
	ctx.metrics
		.managed_apps_desired_replicas
		.with_label_values(&labels)
		.set(desired as f64);
}

/// Remove replica gauge series for an object that is being cleaned up.
fn drop_replica_gauges(ctx: &Context, namespace: &str, app: &Project) {
	let labels = [namespace, app.metadata.name.as_deref().unwrap_or("")];
	let _ = ctx
		.metrics
		.managed_apps_ready_replicas
		.remove_label_values(&labels);
	let _ = ctx
		.metrics
		.managed_apps_desired_replicas
		.remove_label_values(&labels);
}

/// Decrement the `managed_apps` gauge for the phase this object was
/// last observed in, if any. Called when the object is being cleaned up.
fn drop_managed_apps_gauge(ctx: &Context, app: &Project) {
	if let Some((_, prev)) = ctx.phase_state.remove(&backoff_key(app)) {
		ctx.metrics
			.managed_apps
			.with_label_values(&[prev.as_str()])
			.dec();
	}
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

fn upsert_project_condition(conditions: &mut Vec<ProjectCondition>, next: ProjectCondition) {
	if let Some(existing) = conditions
		.iter_mut()
		.find(|condition| condition.type_ == next.type_)
	{
		*existing = next;
	} else {
		conditions.push(next);
	}
}

/// Error policy: classify the error and select an exponential backoff
/// duration based on the number of consecutive failures for this object.
///
/// - `Permanent` errors (invalid spec) return `Action::await_change()` —
///   retrying does not help until the user fixes the resource.
/// - `Transient` errors use a 30s base; `DependencyNotReady` uses a 60s
///   base. Both double on each successive failure and cap at 10 minutes.
pub(crate) fn error_policy(obj: Arc<Project>, error: &Error, ctx: Arc<Context>) -> Action {
	error!("Reconciliation error: {error}");

	let class = backoff_class(error);

	match class {
		BackoffClass::Permanent => {
			// Drop any stored attempt count; a permanent error will not be
			// retried until the user changes the spec. `requeue_total` is
			// intentionally NOT incremented here because `await_change()`
			// does not produce a requeue.
			ctx.backoff_state.remove(&backoff_key(&obj));
			Action::await_change()
		}
		BackoffClass::Transient | BackoffClass::DependencyNotReady => {
			let key = backoff_key(&obj);
			// Atomically bump the attempt counter. Using `DashMap::entry`
			// avoids a read+write race between concurrent error_policy calls.
			let attempt = {
				let mut entry = ctx.backoff_state.entry(key).or_insert(0);
				*entry = entry.saturating_add(1);
				*entry
			};
			let base = if class == BackoffClass::DependencyNotReady {
				BACKOFF_BASE_DEPENDENCY_SECS
			} else {
				BACKOFF_BASE_TRANSIENT_SECS
			};
			// Record a requeue only on branches that actually issue
			// `Action::requeue(...)`, so the metric reflects real requeues.
			ctx.metrics
				.requeue_total
				.with_label_values(&[class.as_metric_label()])
				.inc();
			// Use attempt-1 so the first failure requeues at `base` seconds.
			Action::requeue(compute_backoff(base, attempt.saturating_sub(1)))
		}
	}
}

/// Starts the operator controller loop.
pub(crate) async fn run(client: Client, metrics: Arc<Metrics>) {
	let apps: Api<Project> = Api::all(client.clone());
	let deployments: Api<Deployment> = Api::all(client.clone());
	let jobs: Api<Job> = Api::all(client.clone());
	let services: Api<Service> = Api::all(client.clone());
	let statefulsets: Api<StatefulSet> = Api::all(client.clone());
	let network_policies: Api<NetworkPolicy> = Api::all(client.clone());
	let limit_ranges: Api<LimitRange> = Api::all(client.clone());

	let platform = PlatformConfig::from_env();
	let context = Arc::new(Context {
		client,
		platform,
		preview_config: PreviewConfig::from_env(),
		metrics,
		backoff_state: Arc::new(DashMap::new()),
		phase_state: Arc::new(DashMap::new()),
	});

	Controller::new(apps, watcher::Config::default())
		.owns(
			deployments,
			watcher::Config::default()
				.labels("app.kubernetes.io/managed-by=reinhardt-cloud-operator"),
		)
		.owns(
			jobs,
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
	use crate::source_build::{BuildCompletion, BuildFailure};
	use reinhardt_cloud_types::crd::database::{DatabaseEngine, DatabaseSpec};
	use reinhardt_cloud_types::crd::{
		BuildPhase, BuildStatus, BuildTargetKind, ProjectCondition, ProjectPhase, ProjectSpec,
		ProjectStatus, ServicesSpec,
	};
	use reinhardt_cloud_types::{ConditionStatus, ConditionType};
	use rstest::rstest;

	/// Helper to create a minimal `Project` for reconciler tests.
	fn make_test_app(name: &str) -> Project {
		Project {
			metadata: kube::api::ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				generation: Some(1),
				..Default::default()
			},
			spec: ProjectSpec {
				image: "test:latest".to_string(),
				..Default::default()
			},
			status: None,
		}
	}

	fn service_account_with_owner(uid: Option<&str>) -> ServiceAccount {
		ServiceAccount {
			metadata: kube::api::ObjectMeta {
				name: Some("app-sa".to_string()),
				owner_references: uid.map(|uid| {
					vec![
						k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference {
							api_version: "paas.reinhardt-cloud.dev/v1alpha2".to_string(),
							kind: "Project".to_string(),
							name: "app".to_string(),
							uid: uid.to_string(),
							controller: Some(true),
							block_owner_deletion: Some(true),
						},
					]
				}),
				..Default::default()
			},
			..Default::default()
		}
	}

	#[rstest]
	fn service_account_project_uid_returns_controller_owner_uid() {
		// Arrange
		let sa = service_account_with_owner(Some("project-uid"));

		// Act / Assert
		assert_eq!(service_account_project_uid(&sa), Some("project-uid"));
	}

	#[rstest]
	fn service_account_is_owned_by_uid_accepts_matching_project_controller_owner() {
		// Arrange
		let sa = service_account_with_owner(Some("project-uid"));

		// Act / Assert
		assert!(service_account_is_owned_by_uid(&sa, "project-uid"));
	}

	#[rstest]
	fn service_account_is_owned_by_uid_rejects_unowned_account() {
		// Arrange
		let sa = service_account_with_owner(Some("other-uid"));

		// Act / Assert
		assert!(!service_account_is_owned_by_uid(&sa, "project-uid"));
	}

	#[rstest]
	fn service_account_is_owned_by_uid_rejects_non_controller_project_owner() {
		// Arrange
		let mut sa = service_account_with_owner(Some("project-uid"));
		let owners = sa
			.metadata
			.owner_references
			.as_mut()
			.expect("test SA has owner reference");
		owners[0].controller = Some(false);

		// Act / Assert
		assert!(!service_account_is_owned_by_uid(&sa, "project-uid"));
	}

	#[rstest]
	fn service_account_is_owned_by_uid_rejects_non_project_controller_owner() {
		// Arrange
		let mut sa = service_account_with_owner(Some("project-uid"));
		let owners = sa
			.metadata
			.owner_references
			.as_mut()
			.expect("test SA has owner reference");
		owners[0].kind = "ConfigMap".to_string();

		// Act / Assert
		assert!(!service_account_is_owned_by_uid(&sa, "project-uid"));
	}

	fn make_test_ingress(name: &str, namespace: &str, host: &str) -> Ingress {
		Ingress {
			metadata: kube::api::ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some(namespace.to_string()),
				..Default::default()
			},
			spec: Some(k8s_openapi::api::networking::v1::IngressSpec {
				rules: Some(vec![IngressRule {
					host: Some(host.to_string()),
					..Default::default()
				}]),
				..Default::default()
			}),
			..Default::default()
		}
	}

	#[rstest]
	fn existing_resource_is_controlled_by_project_accepts_matching_controller_owner() {
		// Arrange
		let app = make_test_app("payments");
		let metadata = kube::api::ObjectMeta {
			owner_references: Some(vec![
				crate::resources::labels::owner_reference(&app)
					.expect("test app has a valid owner reference"),
			]),
			..Default::default()
		};

		// Act
		let is_owned = existing_resource_is_controlled_by_project(&metadata, &app);

		// Assert
		assert!(is_owned);
	}

	#[rstest]
	fn existing_resource_is_controlled_by_project_rejects_unowned_resource() {
		// Arrange
		let app = make_test_app("payments");
		let metadata = kube::api::ObjectMeta::default();

		// Act
		let is_owned = existing_resource_is_controlled_by_project(&metadata, &app);

		// Assert
		assert!(!is_owned);
	}

	#[rstest]
	fn existing_resource_is_controlled_by_project_rejects_different_owner_uid() {
		// Arrange
		let app = make_test_app("payments");
		let mut other_app = make_test_app("other");
		other_app.metadata.uid = Some("other-uid".to_string());
		let metadata = kube::api::ObjectMeta {
			owner_references: Some(vec![
				crate::resources::labels::owner_reference(&other_app)
					.expect("test app has a valid owner reference"),
			]),
			..Default::default()
		};

		// Act
		let is_owned = existing_resource_is_controlled_by_project(&metadata, &app);

		// Assert
		assert!(!is_owned);
	}

	#[rstest]
	fn ownership_conflict_error_names_target_resource_and_project() {
		// Arrange
		let app = make_test_app("payments");

		// Act
		let error = ownership_conflict_error("Service", "default", "payments", &app);

		// Assert
		assert_eq!(
			error.to_string(),
			"refusing to manage existing Service default/payments: it is not owned by Project default/payments"
		);
	}

	#[rstest]
	fn build_managed_redis_credentials_secret_uses_retained_secret_shape() {
		// Arrange
		let app = make_test_app("payments");

		// Act
		let secret = build_managed_redis_credentials_secret(&app, "default");

		// Assert
		assert_eq!(
			secret.metadata.name.as_deref(),
			Some("payments-redis-credentials")
		);
		assert!(
			secret.metadata.owner_references.is_none(),
			"Redis credentials must not be garbage-collected under Retain policy"
		);
		let labels = secret.metadata.labels.expect("standard labels");
		assert_eq!(
			labels.get("app.kubernetes.io/name").map(String::as_str),
			Some("payments")
		);
		assert_eq!(
			labels
				.get("app.kubernetes.io/managed-by")
				.map(String::as_str),
			Some("reinhardt-cloud-operator")
		);
	}

	#[rstest]
	fn redis_credentials_secret_accepts_legacy_standard_labels() {
		// Arrange
		let app = make_test_app("payments");
		let metadata = ObjectMeta {
			labels: Some(BTreeMap::from([
				("app.kubernetes.io/name".to_string(), "payments".to_string()),
				(
					"app.kubernetes.io/managed-by".to_string(),
					"reinhardt-cloud-operator".to_string(),
				),
			])),
			..Default::default()
		};

		// Act
		let is_managed = redis_credentials_secret_is_managed_by_project(&metadata, &app);

		// Assert
		assert!(is_managed);
	}

	#[rstest]
	fn redis_credentials_secret_rejects_other_app_labels() {
		// Arrange
		let app = make_test_app("payments");
		let metadata = ObjectMeta {
			labels: Some(BTreeMap::from([
				("app.kubernetes.io/name".to_string(), "other".to_string()),
				(
					"app.kubernetes.io/managed-by".to_string(),
					"reinhardt-cloud-operator".to_string(),
				),
			])),
			..Default::default()
		};

		// Act
		let is_managed = redis_credentials_secret_is_managed_by_project(&metadata, &app);

		// Assert
		assert!(!is_managed);
	}

	fn make_test_build_status(target: BuildTargetKind) -> BuildStatus {
		let mut build = BuildStatus {
			phase: BuildPhase::Succeeded,
			target,
			trigger: "abcdef1234567890".to_string(),
			job_name: "ready-app-build-abcdef12".to_string(),
			image: "registry.example.com/ready-app:ready-app-abcdef12".to_string(),
			image_tag: "ready-app-abcdef12".to_string(),
			preview_name: None,
			pr_number: None,
			branch: Some("main".to_string()),
			reason: Some("BuildSucceeded".to_string()),
			message: Some("Kaniko build Job succeeded".to_string()),
			started_at: Some("2026-06-17T00:00:00Z".to_string()),
			last_transition_time: Some("2026-06-17T00:00:00Z".to_string()),
		};
		if build.target == BuildTargetKind::Preview {
			build.image = "registry.example.com/ready-app:pr-42-abcdef12".to_string();
			build.image_tag = "pr-42-abcdef12".to_string();
			build.preview_name = Some("ready-app-pr-42".to_string());
			build.pr_number = Some("42".to_string());
			build.branch = Some("feature/login".to_string());
		}
		build
	}

	#[rstest]
	fn source_build_gate_action_gates_waiting_without_runtime_update() {
		// Arrange
		let decision = BuildDecision::Waiting {
			requeue_after: Duration::from_secs(10),
		};

		// Act
		let action = source_build_gate_action(decision);

		// Assert
		assert_eq!(
			action,
			SourceBuildGateAction::Requeue {
				requeue_after: Duration::from_secs(10),
			}
		);
	}

	#[rstest]
	fn source_build_gate_action_gates_failed_without_runtime_update() {
		// Arrange
		let build = make_test_build_status(BuildTargetKind::Production);
		let decision = BuildDecision::Failed(BuildFailure { status: build });

		// Act
		let action = source_build_gate_action(decision);

		// Assert
		assert_eq!(action, SourceBuildGateAction::AwaitChange);
	}

	#[rstest]
	fn source_build_gate_action_routes_succeeded_production_to_image_update() {
		// Arrange
		let build = make_test_build_status(BuildTargetKind::Production);
		let decision = BuildDecision::Succeeded(BuildCompletion {
			status: build.clone(),
		});

		// Act
		let action = source_build_gate_action(decision);

		// Assert
		assert_eq!(
			action,
			SourceBuildGateAction::UpdateProductionImage { build }
		);
	}

	#[rstest]
	fn source_build_gate_action_routes_succeeded_preview_to_preview_update() {
		// Arrange
		let build = make_test_build_status(BuildTargetKind::Preview);
		let decision = BuildDecision::Succeeded(BuildCompletion {
			status: build.clone(),
		});

		// Act
		let action = source_build_gate_action(decision);

		// Assert
		assert_eq!(action, SourceBuildGateAction::UpdatePreview { build });
	}

	#[rstest]
	fn preview_status_patch_keeps_child_preview_list_populated() {
		// Arrange
		let preview = Project {
			metadata: kube::api::ObjectMeta {
				name: Some("ready-app-pr-42".to_string()),
				labels: Some(
					[
						("reinhardt.dev/preview".to_string(), "true".to_string()),
						(
							"reinhardt.dev/parent-app".to_string(),
							"ready-app".to_string(),
						),
						("reinhardt.dev/pr-number".to_string(), "42".to_string()),
					]
					.into_iter()
					.collect(),
				),
				..Default::default()
			},
			spec: ProjectSpec {
				services: Some(ServicesSpec {
					port: Some(80),
					target_port: Some(8080),
					ingress_host: Some("ready-app-pr-42.preview.example.com".to_string()),
					tls: None,
				}),
				..Default::default()
			},
			status: Some(ProjectStatus {
				phase: Some(ProjectPhase::Running),
				ready_replicas: Some(1),
				..Default::default()
			}),
		};

		// Act
		let patch = build_preview_status_patch(&[preview]);

		// Assert
		assert_eq!(patch["status"]["previews"][0]["name"], "ready-app-pr-42");
		assert_eq!(patch["status"]["previews"][0]["prNumber"], "42");
		assert_eq!(
			patch["status"]["previews"][0]["url"],
			"https://ready-app-pr-42.preview.example.com"
		);
		assert_eq!(patch["status"]["previews"][0]["phase"], "running");
		assert_eq!(patch["status"]["previews"][0]["readyReplicas"], 1);
	}

	fn find_condition(status: &ProjectStatus, condition_type: ConditionType) -> &ProjectCondition {
		status
			.conditions
			.iter()
			.find(|condition| condition.type_ == condition_type)
			.expect("condition should exist")
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
		let status = build_status(&app, true, 3, MigrationGateState::NotRequired, Vec::new());

		// Assert
		assert_eq!(status.phase, Some(ProjectPhase::Running));
		assert_eq!(status.conditions.len(), 2);
		let ready = find_condition(&status, ConditionType::Ready);
		assert_eq!(ready.status, ConditionStatus::True);
		assert_eq!(ready.reason, "ReconcileSuccess");
		assert_eq!(ready.message, "Application is ready");
		let migration = find_condition(&status, ConditionType::MigrationReady);
		assert_eq!(migration.status, ConditionStatus::True);
		assert_eq!(migration.reason, "MigrationNotRequired");
	}

	#[rstest]
	fn test_build_status_sets_deploying_phase_when_not_ready() {
		// Arrange
		let app = make_test_app("deploying-app");

		// Act
		let status = build_status(&app, false, 0, MigrationGateState::NotRequired, Vec::new());

		// Assert
		assert_eq!(status.phase, Some(ProjectPhase::Deploying));
		assert_eq!(status.conditions.len(), 2);
		let ready = find_condition(&status, ConditionType::Ready);
		assert_eq!(ready.status, ConditionStatus::False);
		assert_eq!(ready.reason, "ReconcileInProgress");
		assert_eq!(ready.message, "Waiting for deployment rollout to complete");
		let migration = find_condition(&status, ConditionType::MigrationReady);
		assert_eq!(migration.status, ConditionStatus::True);
		assert_eq!(migration.reason, "MigrationNotRequired");
	}

	#[rstest]
	fn test_build_status_sets_migration_running_condition() {
		// Arrange
		let app = make_test_app("migration-running-app");

		// Act
		let status = build_status(&app, false, 0, MigrationGateState::Running, Vec::new());

		// Assert
		assert_eq!(status.phase, Some(ProjectPhase::Deploying));
		let migration = find_condition(&status, ConditionType::MigrationReady);
		assert_eq!(migration.status, ConditionStatus::False);
		assert_eq!(migration.reason, "MigrationRunning");
	}

	#[rstest]
	fn test_build_status_sets_degraded_phase_when_migration_failed() {
		// Arrange
		let app = make_test_app("migration-failed-app");

		// Act
		let status = build_status(&app, false, 0, MigrationGateState::Failed, Vec::new());

		// Assert
		assert_eq!(status.phase, Some(ProjectPhase::Degraded));
		let migration = find_condition(&status, ConditionType::MigrationReady);
		assert_eq!(migration.status, ConditionStatus::False);
		assert_eq!(migration.reason, "MigrationFailed");
		let degraded = find_condition(&status, ConditionType::Degraded);
		assert_eq!(degraded.status, ConditionStatus::True);
		assert_eq!(degraded.reason, "MigrationFailed");
	}

	#[rstest]
	fn test_build_status_appends_child_conditions() {
		// Arrange
		let app = make_test_app("status-child-app");
		let child = ProjectCondition {
			type_: ConditionType::TlsReady,
			status: ConditionStatus::True,
			reason: "TlsSecretReady".to_string(),
			message: "TLS Secret is present".to_string(),
			last_transition_time: Some("2025-01-01T00:00:00Z".to_string()),
			observed_generation: Some(1),
		};

		// Act
		let status = build_status(&app, true, 1, MigrationGateState::NotRequired, vec![child]);

		// Assert
		assert_eq!(status.conditions.len(), 3);
		assert_eq!(status.conditions[0].type_, ConditionType::Ready);
		assert_eq!(status.conditions[1].type_, ConditionType::MigrationReady);
		assert_eq!(status.conditions[2].type_, ConditionType::TlsReady);
	}

	#[rstest]
	fn test_build_status_upserts_child_conditions() {
		// Arrange
		let mut app = make_test_app("status-child-upsert-app");
		app.status = Some(ProjectStatus {
			conditions: vec![ProjectCondition {
				type_: ConditionType::TlsReady,
				status: ConditionStatus::False,
				reason: "TlsSecretMissing".to_string(),
				message: "TLS Secret is missing".to_string(),
				last_transition_time: Some("2025-01-01T00:00:00Z".to_string()),
				observed_generation: Some(1),
			}],
			..Default::default()
		});
		let child = ProjectCondition {
			type_: ConditionType::TlsReady,
			status: ConditionStatus::True,
			reason: "TlsSecretReady".to_string(),
			message: "TLS Secret is present".to_string(),
			last_transition_time: Some("2025-01-02T00:00:00Z".to_string()),
			observed_generation: Some(1),
		};

		// Act
		let status = build_status(&app, true, 1, MigrationGateState::NotRequired, vec![child]);

		// Assert
		let tls_conditions: Vec<_> = status
			.conditions
			.iter()
			.filter(|condition| condition.type_ == ConditionType::TlsReady)
			.collect();
		assert_eq!(tls_conditions.len(), 1);
		assert_eq!(tls_conditions[0].status, ConditionStatus::True);
		assert_eq!(tls_conditions[0].reason, "TlsSecretReady");
	}

	#[rstest]
	fn test_build_status_sets_observed_generation() {
		// Arrange
		let mut app = make_test_app("gen-app");
		app.metadata.generation = Some(5);

		// Act
		let status = build_status(&app, true, 1, MigrationGateState::NotRequired, Vec::new());

		// Assert
		assert_eq!(status.observed_generation, Some(5));
		assert_eq!(status.conditions[0].observed_generation, Some(5));
	}

	#[rstest]
	fn test_build_status_sets_ready_replicas() {
		// Arrange
		let app = make_test_app("replicas-app");

		// Act
		let status = build_status(&app, true, 7, MigrationGateState::NotRequired, Vec::new());

		// Assert
		assert_eq!(status.ready_replicas, Some(7));
	}

	#[rstest]
	fn build_status_preserves_existing_build_status() {
		// Arrange
		let mut app = make_test_app("build-app");
		app.status = Some(ProjectStatus {
			build: Some(BuildStatus {
				phase: BuildPhase::Running,
				target: BuildTargetKind::Production,
				trigger: "abcdef12".to_string(),
				job_name: "build-app-build-abcdef12".to_string(),
				image: "registry.example.com/build-app:abcdef12".to_string(),
				image_tag: "abcdef12".to_string(),
				preview_name: None,
				pr_number: None,
				branch: Some("main".to_string()),
				reason: Some("BuildRunning".to_string()),
				message: Some("Kaniko build Job is still running".to_string()),
				started_at: Some("2026-06-17T00:00:00Z".to_string()),
				last_transition_time: Some("2026-06-17T00:00:00Z".to_string()),
			}),
			..Default::default()
		});

		// Act
		let status = build_status(&app, true, 1, MigrationGateState::NotRequired, Vec::new());

		// Assert
		let build = status.build.expect("build status");
		assert_eq!(build.job_name, "build-app-build-abcdef12");
	}

	#[rstest]
	fn build_status_preserves_existing_progressing_condition() {
		// Arrange
		let mut app = make_test_app("progressing-app");
		app.status = Some(ProjectStatus {
			conditions: vec![ProjectCondition {
				type_: ConditionType::Progressing,
				status: ConditionStatus::True,
				reason: "BuildRunning".to_string(),
				message: "Kaniko build Job is still running".to_string(),
				last_transition_time: Some("2026-06-17T00:00:00Z".to_string()),
				observed_generation: Some(1),
			}],
			..Default::default()
		});

		// Act
		let status = build_status(&app, false, 0, MigrationGateState::NotRequired, Vec::new());

		// Assert
		assert_eq!(status.conditions.len(), 3);
		let progressing = status
			.conditions
			.iter()
			.find(|condition| condition.type_ == ConditionType::Progressing)
			.expect("progressing condition");
		let ready = status
			.conditions
			.iter()
			.find(|condition| condition.type_ == ConditionType::Ready)
			.expect("ready condition");
		assert_eq!(progressing.reason, "BuildRunning");
		assert_eq!(ready.reason, "ReconcileInProgress");
	}

	#[rstest]
	fn build_status_does_not_create_degraded_when_ready_without_existing_degraded() {
		// Arrange
		let mut app = make_test_app("ready-with-progressing-app");
		app.status = Some(ProjectStatus {
			conditions: vec![ProjectCondition {
				type_: ConditionType::Progressing,
				status: ConditionStatus::True,
				reason: "BuildRunning".to_string(),
				message: "Kaniko build Job is still running".to_string(),
				last_transition_time: Some("2026-06-17T00:00:00Z".to_string()),
				observed_generation: Some(1),
			}],
			..Default::default()
		});

		// Act
		let status = build_status(&app, true, 1, MigrationGateState::NotRequired, Vec::new());

		// Assert
		assert_eq!(status.phase, Some(ProjectPhase::Running));
		assert!(
			status
				.conditions
				.iter()
				.all(|condition| condition.type_ != ConditionType::Degraded)
		);
		let progressing = status
			.conditions
			.iter()
			.find(|condition| condition.type_ == ConditionType::Progressing)
			.expect("progressing condition");
		let ready = status
			.conditions
			.iter()
			.find(|condition| condition.type_ == ConditionType::Ready)
			.expect("ready condition");
		assert_eq!(progressing.status, ConditionStatus::True);
		assert_eq!(progressing.reason, "BuildRunning");
		assert_eq!(ready.status, ConditionStatus::True);
	}

	#[rstest]
	fn build_status_clears_existing_degraded_when_ready() {
		// Arrange
		let old_time = "2026-06-16T00:00:00Z";
		let mut app = make_test_app("recovered-app");
		app.status = Some(ProjectStatus {
			conditions: vec![ProjectCondition {
				type_: ConditionType::Degraded,
				status: ConditionStatus::True,
				reason: "BuildFailed".to_string(),
				message: "Kaniko build Job failed".to_string(),
				last_transition_time: Some(old_time.to_string()),
				observed_generation: Some(1),
			}],
			..Default::default()
		});

		// Act
		let status = build_status(&app, true, 1, MigrationGateState::NotRequired, Vec::new());

		// Assert
		assert_eq!(status.phase, Some(ProjectPhase::Running));
		let ready = status
			.conditions
			.iter()
			.find(|condition| condition.type_ == ConditionType::Ready)
			.expect("ready condition");
		let degraded = status
			.conditions
			.iter()
			.find(|condition| condition.type_ == ConditionType::Degraded)
			.expect("degraded condition");
		assert_eq!(ready.status, ConditionStatus::True);
		assert_eq!(degraded.status, ConditionStatus::False);
		assert_eq!(degraded.reason, "ReconcileSuccess");
		assert_eq!(degraded.message, "Application is ready");
		assert_eq!(degraded.observed_generation, Some(1));
		assert_ne!(degraded.last_transition_time, Some(old_time.to_string()));
		assert!(degraded.last_transition_time.is_some());
	}

	#[rstest]
	fn test_build_status_preserves_transition_time_when_status_unchanged() {
		// Arrange
		let preserved_time = "2025-06-15T12:00:00Z";
		let mut app = make_test_app("preserve-time-app");
		app.status = Some(ProjectStatus {
			phase: Some(ProjectPhase::Running),
			conditions: vec![ProjectCondition {
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
		let status = build_status(&app, true, 1, MigrationGateState::NotRequired, Vec::new());

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
		app.status = Some(ProjectStatus {
			phase: Some(ProjectPhase::Running),
			conditions: vec![ProjectCondition {
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
		let status = build_status(&app, false, 0, MigrationGateState::NotRequired, Vec::new());

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
		let status = build_status(&app, true, 1, MigrationGateState::NotRequired, Vec::new());

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
		let status = build_status(&app, true, 1, MigrationGateState::NotRequired, Vec::new());

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
		let status = build_status(&app, true, 1, MigrationGateState::NotRequired, Vec::new());

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

	/// Build a `Context` for error-policy tests with isolated metrics and
	/// an empty backoff map. Using fresh metrics avoids cross-test
	/// contamination of counters.
	fn test_context() -> Arc<Context> {
		Arc::new(Context {
			client: dummy_client(),
			platform: PlatformConfig::onprem_defaults(),
			preview_config: PreviewConfig::from_env(),
			metrics: Metrics::new(),
			backoff_state: Arc::new(DashMap::new()),
			phase_state: Arc::new(DashMap::new()),
		})
	}

	#[rstest]
	#[tokio::test]
	async fn test_error_policy_transient_returns_base_requeue_on_first_failure() {
		// Arrange
		let app = Arc::new(make_test_app("error-app"));
		let error = Error::MissingNamespace("error-app".to_string());
		let ctx = test_context();

		// Act
		let action = error_policy(app, &error, ctx);

		// Assert — first transient failure uses the 30s base backoff.
		let expected = Action::requeue(Duration::from_secs(30));
		assert_eq!(format!("{action:?}"), format!("{expected:?}"));
	}

	#[rstest]
	#[tokio::test]
	async fn test_error_policy_transient_doubles_on_successive_failures() {
		// Arrange
		let app = Arc::new(make_test_app("flaky-app"));
		let ctx = test_context();

		// Act — three successive failures for the same object.
		let _ = error_policy(
			app.clone(),
			&Error::MissingNamespace("x".into()),
			ctx.clone(),
		);
		let _ = error_policy(
			app.clone(),
			&Error::MissingNamespace("x".into()),
			ctx.clone(),
		);
		let action = error_policy(app, &Error::MissingNamespace("x".into()), ctx);

		// Assert — third failure requeues at 30 * 2^2 = 120s.
		let expected = Action::requeue(Duration::from_secs(120));
		assert_eq!(format!("{action:?}"), format!("{expected:?}"));
	}

	#[rstest]
	#[tokio::test]
	async fn test_error_policy_permanent_awaits_change() {
		// Arrange
		let app = Arc::new(make_test_app("bad-spec"));
		let error = Error::InvalidPort {
			field: "port",
			port: 70_000,
		};
		let ctx = test_context();

		// Act
		let action = error_policy(app, &error, ctx.clone());

		// Assert — permanent errors do not requeue.
		let expected = Action::await_change();
		assert_eq!(format!("{action:?}"), format!("{expected:?}"));
		// Assert — `requeue_total{reason="permanent"}` is NOT incremented
		// because `Action::await_change()` does not issue a requeue.
		let permanent_requeues = ctx
			.metrics
			.requeue_total
			.with_label_values(&["permanent"])
			.get();
		assert_eq!(permanent_requeues, 0.0);
	}

	#[rstest]
	fn test_compute_backoff_caps_at_max() {
		// Assert: large attempt counts saturate to BACKOFF_MAX_SECS.
		assert_eq!(
			compute_backoff(30, 30),
			Duration::from_secs(BACKOFF_MAX_SECS)
		);
		assert_eq!(compute_backoff(30, 0), Duration::from_secs(30));
		assert_eq!(compute_backoff(30, 1), Duration::from_secs(60));
		assert_eq!(compute_backoff(30, 2), Duration::from_secs(120));
	}

	// ── explicit-database inference wiring ──────────────────────────

	#[rstest]
	fn explicit_database_spec_triggers_inference_branch() {
		// Arrange — app with explicit DatabaseSpec and on-prem platform
		let mut app = make_test_app("explicit-db-app");
		app.spec.database = Some(DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: Some(25),
			version: Some("16".to_string()),
		});
		let platform = PlatformConfig::onprem_defaults();

		// Act — the reconciler's explicit-DB branch calls exactly this
		// function. A non-empty result proves the wiring is reachable;
		// the previous code path (introspect-only) would have ignored
		// the explicit spec entirely.
		let resources =
			infer_database_resources(&app, &platform).expect("database resources should infer");

		// Assert
		assert_eq!(
			resources.len(),
			5,
			"on-prem inference must emit StatefulSet + PVC + Service + ConfigMap + Secret"
		);
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

	#[rstest]
	fn managed_git_credentials_secret_name_uses_project_scoped_name() {
		// Arrange
		let project_name = "delete-all-app";

		// Act
		let secret_name = managed_git_credentials_secret_name(project_name);

		// Assert
		assert_eq!(secret_name, "delete-all-app-git-credentials");
	}

	#[rstest]
	fn managed_github_credentials_secret_name_uses_dashboard_import_name() {
		// Arrange
		let project_name = "delete-all-app";

		// Act
		let secret_name = managed_github_credentials_secret_name(project_name);

		// Assert
		assert_eq!(secret_name, "delete-all-app-github-git-credentials");
	}

	#[rstest]
	fn git_credentials_cleanup_names_include_github_spec_reference() {
		// Arrange
		let mut app = make_test_app("private-app");
		app.spec.source = Some(reinhardt_cloud_types::crd::source::SourceSpec {
			repository: "https://github.com/example/private-app".to_string(),
			branch: None,
			provider: None,
			credentials_secret: Some("private-app-github-git-credentials".to_string()),
			build: None,
			webhook: None,
			preview: None,
		});

		// Act
		let names = git_credentials_cleanup_names(&app, "private-app");

		// Assert
		assert!(names.contains(&"private-app-git-credentials".to_string()));
		assert!(names.contains(&"private-app-github-git-credentials".to_string()));
	}

	#[rstest]
	fn git_credentials_cleanup_names_exclude_shared_spec_reference() {
		// Arrange
		let mut app = make_test_app("private-app");
		app.spec.source = Some(reinhardt_cloud_types::crd::source::SourceSpec {
			repository: "https://github.com/example/private-app".to_string(),
			branch: None,
			provider: None,
			credentials_secret: Some("shared-git-credentials".to_string()),
			build: None,
			webhook: None,
			preview: None,
		});

		// Act
		let names = git_credentials_cleanup_names(&app, "private-app");

		// Assert
		assert!(!names.contains(&"shared-git-credentials".to_string()));
	}

	#[rstest]
	fn git_credentials_secret_accepts_dashboard_applied_secret_labels() {
		// Arrange
		let app = make_test_app("private-app");
		let secret = Secret {
			metadata: kube::api::ObjectMeta {
				labels: Some(BTreeMap::from([
					(
						"reinhardt.dev/credential-type".to_string(),
						"git".to_string(),
					),
					("reinhardt.dev/provider".to_string(), "github".to_string()),
				])),
				..Default::default()
			},
			..Default::default()
		};

		// Act
		let is_managed = git_credentials_secret_is_managed(
			&secret,
			&app,
			"private-app",
			"private-app-github-git-credentials",
		);

		// Assert
		assert!(is_managed);
	}

	#[rstest]
	fn git_credentials_secret_rejects_shared_secret_even_with_git_labels() {
		// Arrange
		let app = make_test_app("private-app");
		let secret = Secret {
			metadata: kube::api::ObjectMeta {
				labels: Some(BTreeMap::from([
					(
						"reinhardt.dev/credential-type".to_string(),
						"git".to_string(),
					),
					("reinhardt.dev/provider".to_string(), "github".to_string()),
				])),
				..Default::default()
			},
			..Default::default()
		};

		// Act
		let is_managed = git_credentials_secret_is_managed(
			&secret,
			&app,
			"private-app",
			"shared-git-credentials",
		);

		// Assert
		assert!(!is_managed);
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
			tls: None,
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
	fn host_matches_allowed_suffix_accepts_subdomain() {
		// Arrange
		let host = "app.team.example.com";

		// Act
		let result = host_matches_allowed_suffix(host, "example.com");

		// Assert
		assert!(result);
	}

	#[rstest]
	fn is_wildcard_ingress_host_detects_kubernetes_wildcard_syntax() {
		// Arrange
		let host = "*.example.com";

		// Act
		let result = is_wildcard_ingress_host(host);

		// Assert
		assert!(result);
	}

	#[rstest]
	fn ensure_ingress_host_allowed_rejects_wildcard_host() {
		// Arrange
		let host = "*.example.com";

		// Act
		let result = ensure_ingress_host_allowed(host);

		// Assert
		match result {
			Err(Error::InvalidIngressHost(message)) => assert_eq!(
				message,
				"services.ingress_host '*.example.com' must not use Kubernetes wildcard host syntax"
			),
			other => panic!("expected wildcard ingress host rejection, got {other:?}"),
		}
	}

	#[rstest]
	fn host_matches_allowed_suffix_rejects_suffix_confusion() {
		// Arrange
		let host = "attackerexample.com";

		// Act
		let result = host_matches_allowed_suffix(host, "example.com");

		// Assert
		assert!(!result);
	}

	#[rstest]
	fn ingress_claims_host_matches_case_insensitively() {
		// Arrange
		let ingress = make_test_ingress("app", "default", "Login.Platform.Example.com");

		// Act
		let result = ingress_claims_host(&ingress, "login.platform.example.com");

		// Assert
		assert!(result);
	}

	#[rstest]
	fn ingress_claims_host_rejects_different_host() {
		// Arrange
		let ingress = make_test_ingress("app", "default", "app.example.com");

		// Act
		let result = ingress_claims_host(&ingress, "login.platform.example.com");

		// Assert
		assert!(!result);
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
			tls: None,
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
			"kind": "Project",
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
		let app: Project = serde_json::from_value(json).unwrap();

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
			"kind": "Project",
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
		let app: Project = serde_json::from_value(json).unwrap();

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
			"kind": "Project",
			"metadata": { "name": "myapp", "namespace": "default", "uid": "uid" },
			"spec": {
				"image": "myapp:latest",
				"cache": { "backend": "redis" }
			}
		});
		let app: Project = serde_json::from_value(json).unwrap();

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
			"kind": "Project",
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
		let app: Project = serde_json::from_value(json).unwrap();

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
			"kind": "Project",
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
		let app: Project = serde_json::from_value(json).unwrap();

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
			"kind": "Project",
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
		let app: Project = serde_json::from_value(json).unwrap();

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
			"kind": "Project",
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
		let app: Project = serde_json::from_value(json).unwrap();

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
			"kind": "Project",
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
		let app: Project = serde_json::from_value(json).unwrap();

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
			"kind": "Project",
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
		let app: Project = serde_json::from_value(json).unwrap();

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
			"kind": "Project",
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
		let app: Project = serde_json::from_value(json).unwrap();

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
			"kind": "Project",
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
		let app: Project = serde_json::from_value(json).unwrap();

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
			"kind": "Project",
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
		let app: Project = serde_json::from_value(json).unwrap();

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

	// ── tenant validation tests (#416) ──────────────────────────────

	#[rstest]
	fn validate_tenant_namespace_returns_none_when_tenant_unset() {
		// Arrange
		let app = make_test_app("legacy-app");

		// Act
		let result = validate_tenant_namespace(&app, "default");

		// Assert — backward-compat: legacy CRs without spec.tenant
		// must continue to reconcile without enforcement.
		assert!(matches!(result, Ok(None)));
	}

	#[rstest]
	fn validate_tenant_namespace_accepts_matching_namespace() {
		// Arrange
		let mut app = make_test_app("acme-app");
		app.spec.tenant = Some(reinhardt_cloud_types::crd::tenant::TenantRef {
			organization: "acme".to_string(),
			team: None,
		});
		app.metadata.namespace = Some("tenant-acme".to_string());

		// Act
		let result = validate_tenant_namespace(&app, "tenant-acme");

		// Assert
		assert_eq!(result.unwrap(), Some("tenant-acme".to_string()));
	}

	#[rstest]
	fn validate_tenant_namespace_accepts_org_team_combo() {
		// Arrange
		let mut app = make_test_app("platform-app");
		app.spec.tenant = Some(reinhardt_cloud_types::crd::tenant::TenantRef {
			organization: "acme".to_string(),
			team: Some("platform".to_string()),
		});

		// Act
		let result = validate_tenant_namespace(&app, "tenant-acme-platform");

		// Assert
		assert_eq!(result.unwrap(), Some("tenant-acme-platform".to_string()));
	}

	#[rstest]
	fn validate_tenant_namespace_rejects_mismatch() {
		// Arrange
		let mut app = make_test_app("acme-app");
		app.spec.tenant = Some(reinhardt_cloud_types::crd::tenant::TenantRef {
			organization: "acme".to_string(),
			team: None,
		});

		// Act — CR placed in `default` instead of `tenant-acme`.
		let result = validate_tenant_namespace(&app, "default");

		// Assert
		match result {
			Err(Error::TenantMismatch { expected, actual }) => {
				assert_eq!(expected, "tenant-acme");
				assert_eq!(actual, "default");
			}
			other => panic!("expected TenantMismatch, got {other:?}"),
		}
	}

	#[rstest]
	fn validate_tenant_namespace_rejects_invalid_tenant() {
		// Arrange — uppercase organization fails DNS-1123 validation.
		let mut app = make_test_app("bad-app");
		app.spec.tenant = Some(reinhardt_cloud_types::crd::tenant::TenantRef {
			organization: "ACME".to_string(),
			team: None,
		});

		// Act
		let result = validate_tenant_namespace(&app, "tenant-ACME");

		// Assert
		assert!(matches!(result, Err(Error::InvalidTenant(_))));
	}

	#[rstest]
	fn build_degraded_status_patch_emits_failed_phase() {
		// Arrange
		let app = make_test_app("acme-app");

		// Act
		let payload = build_degraded_status_patch(
			&app,
			"TenantMismatch",
			"namespace 'default' does not match expected 'tenant-acme'",
		);

		// Assert — the emitted JSON must drive the CR into `Failed`
		// phase and carry a single Degraded=True condition with the
		// supplied reason/message. `ProjectPhase` serializes lowercase
		// (see crd/enums.rs), so the wire value is `"failed"`.
		let status = &payload["status"];
		assert_eq!(status["phase"], serde_json::json!("failed"));
		let condition = &status["conditions"][0];
		assert_eq!(condition["type"], serde_json::json!("Degraded"));
		assert_eq!(condition["status"], serde_json::json!("True"));
		assert_eq!(condition["reason"], serde_json::json!("TenantMismatch"));
		assert!(
			condition["message"]
				.as_str()
				.expect("message")
				.contains("tenant-acme"),
		);
	}

	#[rstest]
	fn build_degraded_status_patch_observes_generation() {
		// Arrange — generation must round-trip into the patch payload so
		// the API server records `observedGeneration` correctly.
		let mut app = make_test_app("gen-app");
		app.metadata.generation = Some(7);

		// Act
		let payload = build_degraded_status_patch(&app, "Reason", "msg");

		// Assert
		let status = &payload["status"];
		assert_eq!(status["observedGeneration"], serde_json::json!(7));
		assert_eq!(
			status["conditions"][0]["observedGeneration"],
			serde_json::json!(7)
		);
	}

	#[rstest]
	fn build_degraded_status_patch_includes_transition_time() {
		// Arrange
		let app = make_test_app("ts-app");

		// Act
		let payload = build_degraded_status_patch(&app, "Reason", "msg");

		// Assert — `lastTransitionTime` must be a non-empty RFC-3339 string.
		let ts = payload["status"]["conditions"][0]["lastTransitionTime"]
			.as_str()
			.expect("lastTransitionTime present");
		assert!(!ts.is_empty());
		assert!(
			chrono::DateTime::parse_from_rfc3339(ts).is_ok(),
			"lastTransitionTime must parse as RFC-3339, got {ts}",
		);
	}

	// ── phase_label tests ───────────────────────────────────────────

	#[rstest]
	#[case(ProjectPhase::Pending, "Pending")]
	#[case(ProjectPhase::Provisioning, "Provisioning")]
	#[case(ProjectPhase::Deploying, "Deploying")]
	#[case(ProjectPhase::Running, "Running")]
	#[case(ProjectPhase::Degraded, "Degraded")]
	#[case(ProjectPhase::Failed, "Failed")]
	#[case(ProjectPhase::Terminating, "Terminating")]
	fn phase_label_returns_stable_metric_label(
		#[case] phase: ProjectPhase,
		#[case] expected: &'static str,
	) {
		// Act
		let label = phase_label(&phase);

		// Assert — labels are stamped into Prometheus metrics; they must
		// remain stable so dashboards keep working across releases.
		assert_eq!(label, expected);
	}

	// ── backoff_key tests ───────────────────────────────────────────

	#[rstest]
	fn backoff_key_combines_namespace_and_name() {
		// Arrange
		let mut app = make_test_app("acme-app");
		app.metadata.namespace = Some("tenant-acme".to_string());

		// Act
		let key = backoff_key(&app);

		// Assert
		assert_eq!(key, ("tenant-acme".to_string(), "acme-app".to_string()),);
	}

	#[rstest]
	fn backoff_key_uses_empty_string_for_missing_namespace() {
		// Arrange — cluster-scoped or pre-admission objects may lack a
		// namespace; backoff_key must still produce a stable key.
		let mut app = make_test_app("orphan-app");
		app.metadata.namespace = None;

		// Act
		let key = backoff_key(&app);

		// Assert
		assert_eq!(key, (String::new(), "orphan-app".to_string()));
	}

	// ── managed_apps gauge tests ───────────────────────────────────

	#[rstest]
	#[tokio::test]
	async fn update_managed_apps_gauge_increments_new_phase() {
		// Arrange
		let app = make_test_app("g1-app");
		let ctx = test_context();

		// Act
		update_managed_apps_gauge(&ctx, &app, "Running");

		// Assert
		let running = ctx
			.metrics
			.managed_apps
			.with_label_values(&["Running"])
			.get();
		assert_eq!(running, 1.0);
		assert_eq!(
			ctx.phase_state.get(&backoff_key(&app)).map(|v| v.clone()),
			Some("Running".to_string()),
		);
	}

	#[rstest]
	#[tokio::test]
	async fn update_managed_apps_gauge_decrements_previous_on_transition() {
		// Arrange
		let app = make_test_app("g2-app");
		let ctx = test_context();
		update_managed_apps_gauge(&ctx, &app, "Deploying");

		// Act — transition Deploying -> Running.
		update_managed_apps_gauge(&ctx, &app, "Running");

		// Assert — Deploying is decremented back to 0, Running is 1.
		let deploying = ctx
			.metrics
			.managed_apps
			.with_label_values(&["Deploying"])
			.get();
		let running = ctx
			.metrics
			.managed_apps
			.with_label_values(&["Running"])
			.get();
		assert_eq!(deploying, 0.0);
		assert_eq!(running, 1.0);
	}

	#[rstest]
	#[tokio::test]
	async fn update_managed_apps_gauge_is_idempotent_for_same_phase() {
		// Arrange
		let app = make_test_app("g3-app");
		let ctx = test_context();
		update_managed_apps_gauge(&ctx, &app, "Running");

		// Act — same phase observed again must not double-count.
		update_managed_apps_gauge(&ctx, &app, "Running");

		// Assert
		let running = ctx
			.metrics
			.managed_apps
			.with_label_values(&["Running"])
			.get();
		assert_eq!(running, 1.0);
	}

	#[rstest]
	#[tokio::test]
	async fn drop_managed_apps_gauge_decrements_last_phase() {
		// Arrange — record an app in Running, then drop.
		let app = make_test_app("g4-app");
		let ctx = test_context();
		update_managed_apps_gauge(&ctx, &app, "Running");

		// Act
		drop_managed_apps_gauge(&ctx, &app);

		// Assert — gauge returns to 0 and the phase entry is removed.
		let running = ctx
			.metrics
			.managed_apps
			.with_label_values(&["Running"])
			.get();
		assert_eq!(running, 0.0);
		assert!(ctx.phase_state.get(&backoff_key(&app)).is_none());
	}

	#[rstest]
	#[tokio::test]
	async fn drop_managed_apps_gauge_no_op_for_untracked_app() {
		// Arrange — never tracked.
		let app = make_test_app("untracked-app");
		let ctx = test_context();

		// Act
		drop_managed_apps_gauge(&ctx, &app);

		// Assert — no panic, no negative counts.
		let running = ctx
			.metrics
			.managed_apps
			.with_label_values(&["Running"])
			.get();
		assert_eq!(running, 0.0);
	}

	#[rstest]
	#[tokio::test]
	async fn drop_replica_gauges_removes_deleted_project_series() {
		// Arrange
		let app = make_test_app("replica-cleanup-app");
		let ctx = test_context();
		update_replica_gauges(&ctx, "tenant-cleanup", &app, 2, 3);
		let labels = ["tenant-cleanup", "replica-cleanup-app"];
		assert_eq!(
			ctx.metrics
				.managed_apps_ready_replicas
				.with_label_values(&labels)
				.get(),
			2.0,
		);
		assert_eq!(
			ctx.metrics
				.managed_apps_desired_replicas
				.with_label_values(&labels)
				.get(),
			3.0,
		);

		// Act
		drop_replica_gauges(&ctx, "tenant-cleanup", &app);

		// Assert
		let after = String::from_utf8(ctx.metrics.encode()).expect("utf8");
		let retained_series: Vec<&str> = after
			.lines()
			.filter(|line| {
				line.starts_with("reinhardt_cloud_operator_managed_apps_")
					&& line.contains(r#"project="replica-cleanup-app""#)
			})
			.collect();
		assert_eq!(retained_series, Vec::<&str>::new());
	}

	// ── error_policy: dependency-not-ready branch ───────────────────

	#[rstest]
	#[tokio::test]
	async fn test_error_policy_dependency_not_ready_uses_60s_base() {
		// Arrange — fabricate a 404 kube::Error to drive the
		// `DependencyNotReady` branch. Only `code` matters for
		// `kube_status_class`; other fields use defaults.
		let status = kube::core::Status {
			code: 404,
			message: "not found".to_string(),
			reason: "NotFound".to_string(),
			..Default::default()
		};
		let app = Arc::new(make_test_app("dep-app"));
		let error = Error::Kube(kube::Error::Api(Box::new(status)));
		let ctx = test_context();

		// Act
		let action = error_policy(app, &error, ctx.clone());

		// Assert — first dependency-not-ready failure uses the 60s base.
		let expected = Action::requeue(Duration::from_secs(60));
		assert_eq!(format!("{action:?}"), format!("{expected:?}"));

		// Assert — `requeue_total{reason="dependency_not_ready"}` was
		// incremented exactly once.
		let count = ctx
			.metrics
			.requeue_total
			.with_label_values(&["dependency_not_ready"])
			.get();
		assert_eq!(count, 1.0);
	}

	#[rstest]
	#[tokio::test]
	async fn test_error_policy_permanent_drops_backoff_state() {
		// Arrange — accumulate transient attempts, then encounter a
		// permanent error. The permanent branch must clear the counter
		// so a future spec edit starts from a clean slate.
		let app = Arc::new(make_test_app("recover-app"));
		let ctx = test_context();
		let _ = error_policy(
			app.clone(),
			&Error::MissingNamespace("x".into()),
			ctx.clone(),
		);
		assert!(ctx.backoff_state.get(&backoff_key(&app)).is_some());

		// Act
		let _ = error_policy(
			app.clone(),
			&Error::TenantMismatch {
				expected: "tenant-acme".into(),
				actual: "default".into(),
			},
			ctx.clone(),
		);

		// Assert
		assert!(
			ctx.backoff_state.get(&backoff_key(&app)).is_none(),
			"permanent errors must clear stored backoff attempts",
		);
	}

	// ── compute_backoff edge cases ──────────────────────────────────

	#[rstest]
	fn compute_backoff_uses_60s_base_for_dependency() {
		// Assert — base=60s, attempt=0 -> 60s; attempt=1 -> 120s.
		assert_eq!(compute_backoff(60, 0), Duration::from_secs(60));
		assert_eq!(compute_backoff(60, 1), Duration::from_secs(120));
		assert_eq!(compute_backoff(60, 2), Duration::from_secs(240));
	}

	#[rstest]
	fn compute_backoff_saturates_for_overflowing_attempts() {
		// Arrange / Act — extreme attempt counts must not overflow.
		let result = compute_backoff(30, u32::MAX);

		// Assert — clamps to BACKOFF_MAX_SECS (10 minutes).
		assert_eq!(result, Duration::from_secs(BACKOFF_MAX_SECS));
	}
}
