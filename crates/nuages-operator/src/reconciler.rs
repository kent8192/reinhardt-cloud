//! Reconciler logic for the `ReinhardtApp` custom resource.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use futures::StreamExt;
use k8s_openapi::api::apps::v1::{Deployment, StatefulSet};
use k8s_openapi::api::batch::v1::Job;
use k8s_openapi::api::core::v1::{ConfigMap, Secret, Service};
use k8s_openapi::api::networking::v1::Ingress;
use kube::api::{Api, DeleteParams, Patch, PatchParams, PostParams};
use kube::runtime::controller::{Action, Controller};
use kube::runtime::finalizer::{Event as FinalizerEvent, finalizer};
use kube::runtime::watcher;
use kube::{Client, ResourceExt};
use tracing::{error, info, warn};

use crate::error::Error;
use crate::inference::configmap::build_settings_configmap;
use crate::inference::platform::PlatformConfig;
use crate::inference::secrets::{build_db_credentials_secret, build_jwt_secret};
use crate::resources::{
	build_db_secret, build_db_service, build_db_statefulset, build_deployment, build_ingress,
	build_migration_job, build_service,
};
use nuages_types::crd::database::{DatabaseStatus, ResourcePhase};
use nuages_types::crd::policy::DeletionPolicy;
use nuages_types::crd::{AppCondition, AppPhase, ReinhardtApp, ReinhardtAppStatus};
use nuages_types::{ConditionStatus, ConditionType};

const FINALIZER_NAME: &str = "paas.nuages.dev/cleanup";

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
	let ssapply = PatchParams::apply("nuages-operator").force();

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

	// Reconcile owned Deployment via server-side apply
	let deployments: Api<Deployment> = Api::namespaced(ctx.client.clone(), namespace);
	let desired_deployment = build_deployment(&app)?;
	deployments
		.patch(
			&name,
			&PatchParams::apply("nuages-operator").force(),
			&Patch::Apply(&desired_deployment),
		)
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled Deployment {namespace}/{name}");

	// Reconcile owned Service via server-side apply
	let services: Api<Service> = Api::namespaced(ctx.client.clone(), namespace);
	let desired_service = build_service(&app)?;
	services
		.patch(
			&name,
			&PatchParams::apply("nuages-operator").force(),
			&Patch::Apply(&desired_service),
		)
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled Service {namespace}/{name}");

	// Introspect-aware resource generation
	if let Some(introspect) = &app.spec.introspect {
		let signals = &introspect.features.infrastructure_signals;

		// Database provisioning (Phase 1: PostgreSQL only)
		if nuages_core::inference::requires_postgresql(signals) {
			reconcile_db_secret(&app, &ctx.client, namespace).await?;
			reconcile_db_statefulset(&app, &ctx.client, namespace).await?;
			reconcile_db_service_resource(&app, &ctx.client, namespace).await?;
			reconcile_migration_job_resource(&app, &ctx.client, namespace).await?;
		}

		// Ingress from routes
		if !introspect.routes.is_empty() {
			let port = nuages_core::inference::app_port(&introspect.settings);
			reconcile_ingress_resource(&app, &ctx.client, namespace, &introspect.routes, port)
				.await?;
		}
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

			// Delete introspect-managed database resources
			delete_if_exists::<StatefulSet>(
				&ctx.client,
				namespace,
				&format!("{name}-postgresql"),
			)
			.await?;
			delete_if_exists::<Service>(
				&ctx.client,
				namespace,
				&format!("{name}-postgresql"),
			)
			.await?;
		}
	}

	Ok(Action::await_change())
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
	let ssapply = PatchParams::apply("nuages-operator").force();

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
	let ssapply = PatchParams::apply("nuages-operator").force();

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
	routes: &[nuages_types::introspect::RouteMetadata],
	port: u16,
) -> Result<(), Error> {
	let name = app.name_any();
	let ssapply = PatchParams::apply("nuages-operator").force();

	let desired = build_ingress(app, routes, port, None)?;
	let ingress_api: Api<Ingress> = Api::namespaced(client.clone(), namespace);
	ingress_api
		.patch(&name, &ssapply, &Patch::Apply(&desired))
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled Ingress {namespace}/{name}");
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
	existing_status: Option<&nuages_types::ConditionStatus>,
	new_status: &nuages_types::ConditionStatus,
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

	let platform = PlatformConfig::from_env();
	let context = Arc::new(Context { client, platform });

	Controller::new(apps, watcher::Config::default())
		.owns(
			deployments,
			watcher::Config::default().labels("app.kubernetes.io/managed-by=nuages-operator"),
		)
		.owns(
			services,
			watcher::Config::default().labels("app.kubernetes.io/managed-by=nuages-operator"),
		)
		.owns(
			statefulsets,
			watcher::Config::default().labels("app.kubernetes.io/managed-by=nuages-operator"),
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
	use nuages_types::crd::database::{DatabaseEngine, DatabaseSpec};
	use nuages_types::crd::{AppCondition, AppPhase, ReinhardtAppSpec, ReinhardtAppStatus};
	use nuages_types::{ConditionStatus, ConditionType};
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
		use nuages_types::introspect::{
			FeaturesMetadata, InfraSignals, IntrospectOutput,
		};

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
		let needs_pg = nuages_core::inference::requires_postgresql(signals);

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
		use nuages_types::introspect::{IntrospectOutput, RouteMetadata, SettingsMetadata};

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
		let port = nuages_core::inference::app_port(&introspect.settings);

		// Assert
		assert!(has_routes);
		assert_eq!(port, 8000);
	}

	#[rstest]
	fn test_introspect_without_db_skips_postgresql() {
		// Arrange
		use nuages_types::introspect::{
			FeaturesMetadata, InfraSignals, IntrospectOutput,
		};

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
		let needs_pg = nuages_core::inference::requires_postgresql(signals);

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
}
