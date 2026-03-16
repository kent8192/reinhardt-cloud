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
use nuages_types::crd::ReinhardtApp;

const FINALIZER_NAME: &str = "paas.nuages.dev/cleanup";

/// Shared context available to every reconciliation call.
pub(crate) struct Context {
	/// Kubernetes API client.
	pub client: Client,
}

/// Main reconciliation entry point.
pub(crate) async fn reconcile(obj: Arc<ReinhardtApp>, ctx: Arc<Context>) -> Result<Action, Error> {
	let name = obj.name_any();
	let namespace = obj.namespace().unwrap_or_default();
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
	let desired_deployment = build_deployment(&app, namespace);
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
	let desired_service = build_service(&app, namespace);
	services
		.patch(
			&name,
			&PatchParams::apply("nuages-operator").force(),
			&Patch::Apply(&desired_service),
		)
		.await
		.map_err(Error::Kube)?;
	info!("Reconciled Service {namespace}/{name}");

	// Update status sub-resource
	update_status(&app, &ctx.client, namespace, true).await?;

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

/// Updates the status sub-resource of a `ReinhardtApp`.
async fn update_status(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
	ready: bool,
) -> Result<(), Error> {
	let api: Api<ReinhardtApp> = Api::namespaced(client.clone(), namespace);
	let phase = if ready { "Running" } else { "Failed" };
	let condition_status = if ready { "True" } else { "False" };
	let reason = if ready {
		"ReconcileSuccess"
	} else {
		"ReconcileError"
	};
	let message = if ready {
		"Application is ready"
	} else {
		"Reconciliation failed"
	};

	let status = serde_json::json!({
		"status": {
			"phase": phase,
			"observedGeneration": app.metadata.generation,
			"conditions": [{
				"type": "Ready",
				"status": condition_status,
				"reason": reason,
				"message": message,
				"lastTransitionTime": chrono::Utc::now().to_rfc3339(),
			}]
		}
	});

	api.patch_status(
		&app.name_any(),
		&PatchParams::default(),
		&Patch::Merge(status),
	)
	.await
	.map_err(Error::Kube)?;

	Ok(())
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
