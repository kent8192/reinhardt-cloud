//! Reconciler logic for the `ReinhardtApp` custom resource.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use k8s_openapi::api::apps::v1::Deployment;
use k8s_openapi::api::core::v1::Service;
use kube::api::{Api, Patch, PatchParams};
use kube::runtime::controller::{Action, Controller};
use kube::runtime::finalizer::{Event as FinalizerEvent, finalizer};
use kube::runtime::watcher;
use kube::{Client, ResourceExt};
use tracing::{error, info};

use crate::error::Error;
use crate::resources::{build_deployment, build_service};
use nuages_types::crd::{AppCondition, AppPhase, ReinhardtApp, ReinhardtAppStatus};
use nuages_types::{ConditionStatus, ConditionType};

const FINALIZER_NAME: &str = "paas.nuages.dev/cleanup";

/// Shared context available to every reconciliation call.
pub(crate) struct Context {
	/// Kubernetes API client.
	pub client: Client,
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
async fn cleanup(
	app: Arc<ReinhardtApp>,
	_ctx: &Context,
	_namespace: &str,
) -> Result<Action, Error> {
	let name = app.name_any();
	info!("Cleaning up ReinhardtApp {name}");
	// Owned Deployments and Services are garbage-collected via ownerReferences.
	// Add cleanup for external resources (databases, DNS, etc.) here.
	Ok(Action::await_change())
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

	let context = Arc::new(Context { client });

	Controller::new(apps, watcher::Config::default())
		.owns(
			deployments,
			watcher::Config::default().labels("app.kubernetes.io/managed-by=nuages-operator"),
		)
		.owns(
			services,
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
	use nuages_types::crd::{AppCondition, AppPhase, ReinhardtAppSpec, ReinhardtAppStatus};
	use nuages_types::{ConditionStatus, ConditionType};
	use rstest::rstest;
	use std::collections::BTreeMap;

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
				replicas: None,
				database: None,
				cache: None,
				worker: None,
				auth: None,
				storage: None,
				mail: None,
				scale: None,
				health: None,
				services: None,
				deletion_policy: Default::default(),
				features: vec![],
				env: BTreeMap::new(),
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

	#[rstest]
	#[tokio::test]
	async fn test_error_policy_returns_requeue_action() {
		// Arrange
		let app = Arc::new(make_test_app("error-app"));
		let error = Error::MissingNamespace("error-app".to_string());
		let ctx = Arc::new(Context {
			client: dummy_client(),
		});

		// Act
		let action = error_policy(app, &error, ctx);

		// Assert — error_policy must return a 30-second requeue
		let expected = Action::requeue(Duration::from_secs(30));
		assert_eq!(format!("{action:?}"), format!("{expected:?}"));
	}
}
