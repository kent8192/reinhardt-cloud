# Kubernetes Operator Patterns

## Purpose

This file defines Kubernetes operator patterns for the Nuages project using kube-rs. These rules ensure consistent, production-ready operator implementations across all Nuages crates.

---

## CRD Design

### CD-1 (MUST): Strongly Typed CRD Definitions

ALL Kubernetes resources MUST be defined as proper CRD types using `#[derive(CustomResource)]`. CRD types MUST implement the required kube-rs traits. ALWAYS use structured types for spec/status, never raw `serde_json::Value`.

**DON'T:**

```rust
// ❌ Untyped spec loses compile-time safety
pub struct ReinhardtAppSpec {
	pub config: serde_json::Value,
}
```

**DO:**

```rust
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(CustomResource, Debug, Serialize, Deserialize, Clone, JsonSchema)]
#[kube(
	group = "paas.nuages.dev",
	version = "v1alpha1",
	kind = "ReinhardtApp",
	namespaced,
	status = "ReinhardtAppStatus",
	printcolumn = r#"{"name":"Image","type":"string","jsonPath":".spec.image"}"#,
	printcolumn = r#"{"name":"Replicas","type":"integer","jsonPath":".spec.replicas"}"#,
	printcolumn = r#"{"name":"Ready","type":"string","jsonPath":".status.conditions[?(@.type==\"Ready\")].status"}"#
)]
pub struct ReinhardtAppSpec {
	/// Docker image to deploy (e.g., `myapp:latest`)
	pub image: String,
	/// Number of desired replicas (defaults to 1)
	pub replicas: Option<i32>,
	/// Resource requests and limits for the application container
	pub resources: Option<ResourceRequirements>,
	/// Environment variables to inject into the application container
	#[serde(default)]
	pub env: Vec<EnvVar>,
}

#[derive(Debug, Serialize, Deserialize, Clone, JsonSchema, Default)]
pub struct ReinhardtAppStatus {
	/// Standard Kubernetes condition list
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub conditions: Vec<Condition>,
	/// The generation last observed by the controller
	pub observed_generation: Option<i64>,
	/// Number of ready replicas
	pub ready_replicas: Option<i32>,
}
```

**Why?** Raw `serde_json::Value` eliminates compile-time type checking, making bugs harder to detect and refactoring risky.

### CD-2 (MUST): CRD Versioning

- ALWAYS use a versioned API group (e.g., `v1alpha1`, `v1beta1`, `v1`)
- CRD group MUST follow the format: `<subdomain>.<domain>` (e.g., `paas.nuages.dev`)
- Progress through versions: `v1alpha1` → `v1beta1` → `v1` as stability increases
- Breaking schema changes MUST bump the version

### CD-3 (SHOULD): CRD Print Columns

Add `printcolumn` annotations to show useful information in `kubectl get` output:

```rust
#[kube(
	printcolumn = r#"{"name":"Image","type":"string","jsonPath":".spec.image"}"#,
	printcolumn = r#"{"name":"Ready","type":"string","jsonPath":".status.conditions[?(@.type==\"Ready\")].status"}"#,
	printcolumn = r#"{"name":"Age","type":"date","jsonPath":".metadata.creationTimestamp"}"#
)]
```

---

## Reconciler Pattern

### RP-1 (MUST): Pure Reconciler Function Signature

MUST implement the reconciler as a pure async function:

```rust
async fn reconcile(obj: Arc<ReinhardtApp>, ctx: Arc<Context>) -> Result<Action>
```

**Requirements:**
- The reconciler function MUST be async
- ALWAYS use `Arc<T>` for the object and context parameters
- MUST return `Result<Action>` (never panic)
- ALWAYS use finalizers for cleanup of external resources
- MUST handle reconciler errors with `error_policy` returning `Action::requeue(duration)`

**Example:**

```rust
use kube::{
	api::{Api, Patch, PatchParams},
	runtime::controller::{Action, Controller},
	Client, ResourceExt,
};
use std::{sync::Arc, time::Duration};

pub struct Context {
	pub client: Client,
}

async fn reconcile(obj: Arc<ReinhardtApp>, ctx: Arc<Context>) -> Result<Action, Error> {
	let name = obj.name_any();
	let namespace = obj.namespace().unwrap_or_default();

	// Ensure finalizer is set before any external resource creation
	let obj = ensure_finalizer(obj, &ctx.client).await?;

	// Reconcile owned Deployment
	reconcile_deployment(&obj, &ctx.client, &namespace).await?;

	// Reconcile owned Service
	reconcile_service(&obj, &ctx.client, &namespace).await?;

	// Update status
	update_status(&obj, &ctx.client, &namespace).await?;

	// Return await_change for steady state (no periodic reconciliation needed)
	Ok(Action::await_change())
}

fn error_policy(
	_obj: Arc<ReinhardtApp>,
	error: &Error,
	_ctx: Arc<Context>,
) -> Action {
	// Requeue with exponential backoff for transient errors
	eprintln!("Reconciliation error: {error}");
	Action::requeue(Duration::from_secs(30))
}
```

### RP-2 (MUST): Finalizer Usage for External Resources

ALWAYS use finalizers when the reconciler creates resources outside of Kubernetes (e.g., cloud databases, DNS records, external load balancers).

```rust
use kube::runtime::finalizer::{finalizer, Event as FinalizerEvent};

const FINALIZER_NAME: &str = "paas.nuages.dev/cleanup";

async fn reconcile(obj: Arc<ReinhardtApp>, ctx: Arc<Context>) -> Result<Action, Error> {
	let api: Api<ReinhardtApp> = Api::namespaced(
		ctx.client.clone(),
		&obj.namespace().unwrap_or_default(),
	);

	finalizer(&api, FINALIZER_NAME, obj, |event| async {
		match event {
			FinalizerEvent::Apply(obj) => apply_reconcile(obj, &ctx).await,
			FinalizerEvent::Cleanup(obj) => cleanup_external_resources(obj, &ctx).await,
		}
	})
	.await
	.map_err(|e| Error::FinalizerError(Box::new(e)))
}
```

### RP-3 (SHOULD): Idempotent Reconciliation

Reconciler logic MUST be idempotent — running the same reconciliation multiple times MUST produce the same result.

```rust
async fn reconcile_deployment(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
) -> Result<(), Error> {
	let deployments: Api<Deployment> = Api::namespaced(client.clone(), namespace);
	let desired = build_desired_deployment(app);

	// Use server-side apply for idempotent resource management
	deployments
		.patch(
			&app.name_any(),
			&PatchParams::apply("nuages-operator").force(),
			&Patch::Apply(&desired),
		)
		.await
		.map_err(Error::KubeError)?;

	Ok(())
}
```

---

## Controller Structure

### CS-1 (MUST): One Controller Per CRD Type

- MUST have exactly one controller per CRD type
- The controller MUST watch owned resources (Deployment, Service, Ingress) for status changes
- Use `watcher::Config::default()` with appropriate label selectors to limit watch scope

**Example:**

```rust
use kube::runtime::controller::Controller;
use kube::runtime::watcher;

pub async fn run_controller(client: Client) {
	let apps: Api<ReinhardtApp> = Api::all(client.clone());
	let deployments: Api<Deployment> = Api::all(client.clone());
	let services: Api<Service> = Api::all(client.clone());

	let context = Arc::new(Context { client });

	Controller::new(apps, watcher::Config::default())
		// Watch owned Deployments to trigger reconciliation on status changes
		.owns(
			deployments,
			watcher::Config::default().labels("app.kubernetes.io/managed-by=nuages-operator"),
		)
		// Watch owned Services
		.owns(
			services,
			watcher::Config::default().labels("app.kubernetes.io/managed-by=nuages-operator"),
		)
		.shutdown_on_signal()
		.run(reconcile, error_policy, context)
		.for_each(|result| async move {
			match result {
				Ok(obj) => tracing::info!("Reconciled {:?}", obj),
				Err(err) => tracing::error!("Reconciliation error: {:?}", err),
			}
		})
		.await;
}
```

### CS-2 (MUST): Label Convention for Owned Resources

All resources created by the operator MUST carry standard labels:

```rust
fn standard_labels(app: &ReinhardtApp) -> BTreeMap<String, String> {
	BTreeMap::from([
		("app.kubernetes.io/name".to_string(), app.name_any()),
		("app.kubernetes.io/managed-by".to_string(), "nuages-operator".to_string()),
		("app.kubernetes.io/instance".to_string(), app.name_any()),
		(
			"paas.nuages.dev/owner".to_string(),
			format!("{}/{}", app.namespace().unwrap_or_default(), app.name_any()),
		),
	])
}
```

---

## Status Conditions

### SC-1 (MUST): Standard Kubernetes Condition Pattern

ALWAYS update status conditions using the standard Kubernetes condition pattern. MUST set `observedGeneration` in status.

**Standard Condition Types:**
- `Ready` - Whether the application is fully operational
- `Progressing` - Whether a rollout is in progress
- `Degraded` - Whether the application is in a degraded state

**Example:**

```rust
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Condition;
use chrono::Utc;

fn build_ready_condition(ready: bool, message: &str) -> Condition {
	Condition {
		last_transition_time: k8s_openapi::apimachinery::pkg::apis::meta::v1::Time(
			Utc::now(),
		),
		message: message.to_string(),
		observed_generation: None,
		reason: if ready { "ReconcileSuccess".to_string() } else { "ReconcileError".to_string() },
		status: if ready { "True".to_string() } else { "False".to_string() },
		type_: "Ready".to_string(),
	}
}

async fn update_status(
	app: &ReinhardtApp,
	client: &Client,
	namespace: &str,
) -> Result<(), Error> {
	let api: Api<ReinhardtApp> = Api::namespaced(client.clone(), namespace);

	let status = serde_json::json!({
		"status": {
			"observedGeneration": app.metadata.generation,
			"conditions": [
				build_ready_condition(true, "Application is ready"),
			]
		}
	});

	api.patch_status(
		&app.name_any(),
		&PatchParams::default(),
		&Patch::MergePatch(status),
	)
	.await
	.map_err(Error::KubeError)?;

	Ok(())
}
```

### SC-2 (SHOULD): Condition Transition Logic

Only update a condition's `lastTransitionTime` when the condition's `status` changes:

```rust
fn update_condition(
	existing_conditions: &[Condition],
	new_condition: Condition,
) -> Vec<Condition> {
	let mut conditions: Vec<Condition> = existing_conditions.to_vec();

	if let Some(existing) = conditions.iter_mut().find(|c| c.type_ == new_condition.type_) {
		if existing.status != new_condition.status {
			// Status changed — update including lastTransitionTime
			*existing = new_condition;
		} else {
			// Status unchanged — preserve lastTransitionTime, update message only
			existing.message = new_condition.message;
			existing.reason = new_condition.reason;
		}
	} else {
		conditions.push(new_condition);
	}

	conditions
}
```

---

## Error Handling

### EH-1 (MUST): Never Panic in Reconciler

Reconciler MUST NOT panic — return `Err` for transient failures.
Use `Action::requeue(Duration::from_secs(N))` for retryable errors.
Use `Action::await_change()` for terminal states (no retry needed).

**DON'T:**

```rust
async fn reconcile(obj: Arc<ReinhardtApp>, ctx: Arc<Context>) -> Result<Action, Error> {
	let deployment = get_deployment(&obj, &ctx.client).await
		.unwrap();  // ❌ panics on transient error, crashes operator

	Ok(Action::await_change())
}
```

**DO:**

```rust
#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error("Kubernetes API error: {0}")]
	KubeError(#[from] kube::Error),
	#[error("Serialization error: {0}")]
	SerializationError(#[from] serde_json::Error),
	#[error("Finalizer error: {0}")]
	FinalizerError(#[source] Box<dyn std::error::Error + Send + Sync>),
	#[error("Missing field: {0}")]
	MissingField(&'static str),
}

async fn reconcile(obj: Arc<ReinhardtApp>, ctx: Arc<Context>) -> Result<Action, Error> {
	let deployment = get_deployment(&obj, &ctx.client).await
		.map_err(Error::KubeError)?;  // ✅ propagate error, triggers error_policy

	Ok(Action::await_change())
}

fn error_policy(
	_obj: Arc<ReinhardtApp>,
	error: &Error,
	_ctx: Arc<Context>,
) -> Action {
	// Log and requeue — operator continues running
	tracing::error!("Reconciliation failed: {error}");
	Action::requeue(Duration::from_secs(30))  // ✅ requeue after 30s
}
```

### EH-2 (SHOULD): Structured Error Types

Use `thiserror` to define structured error types for the operator:

```rust
#[derive(thiserror::Error, Debug)]
pub enum Error {
	#[error("Kubernetes API error: {0}")]
	KubeError(#[from] kube::Error),

	#[error("Missing required field '{0}' in spec")]
	MissingField(&'static str),

	#[error("Invalid image reference '{0}': {1}")]
	InvalidImageRef(String, String),

	#[error("External resource creation failed: {0}")]
	ExternalResourceError(String),

	#[error("Finalizer error: {0}")]
	FinalizerError(#[source] Box<dyn std::error::Error + Send + Sync>),
}
```

### EH-3 (SHOULD): Action Selection Guide

| Scenario | Recommended Action |
|----------|--------------------|
| Transient error (API unavailable) | `Action::requeue(Duration::from_secs(30))` |
| Rate limited by API server | `Action::requeue(Duration::from_secs(60))` |
| Steady state (no changes needed) | `Action::await_change()` |
| Object being deleted with finalizer | `Action::await_change()` (after cleanup) |
| Permanent error (invalid spec) | `Action::await_change()` (update status, do not retry) |

---

## RBAC

### RB-1 (MUST): Least-Privilege RBAC

The operator's service account MUST follow the principle of least privilege:

```yaml
apiVersion: rbac.authorization.k8s.io/v1
kind: ClusterRole
metadata:
  name: nuages-operator
rules:
  # Full control over owned CRDs
  - apiGroups: ["paas.nuages.dev"]
    resources: ["reinhardtapps"]
    verbs: ["get", "list", "watch", "create", "update", "patch", "delete"]
  - apiGroups: ["paas.nuages.dev"]
    resources: ["reinhardtapps/status"]
    verbs: ["get", "update", "patch"]
  - apiGroups: ["paas.nuages.dev"]
    resources: ["reinhardtapps/finalizers"]
    verbs: ["update"]
  # Read and manage owned Deployments
  - apiGroups: ["apps"]
    resources: ["deployments"]
    verbs: ["get", "list", "watch", "create", "update", "patch", "delete"]
  # Read and manage owned Services
  - apiGroups: [""]
    resources: ["services"]
    verbs: ["get", "list", "watch", "create", "update", "patch", "delete"]
  # Read events (for status reporting)
  - apiGroups: [""]
    resources: ["events"]
    verbs: ["create", "patch"]
```

### RB-2 (MUST): Namespace-Scoped vs Cluster-Scoped

- Prefer `ClusterRole` + `ClusterRoleBinding` only when the operator manages resources across all namespaces
- Use `Role` + `RoleBinding` when the operator is limited to a single namespace
- NEVER grant `cluster-admin` or wildcard (`*`) permissions

---

## Quick Reference

### ✅ MUST DO
- Define ALL Kubernetes resources as proper CRD types with `#[derive(CustomResource)]` (CD-1)
- Use structured types for spec/status — never raw `serde_json::Value` (CD-1)
- Implement reconcilers as pure async functions returning `Result<Action>` (RP-1)
- ALWAYS use finalizers for cleanup of external resources (RP-2)
- MUST NOT panic in reconciler — return `Err` for transient failures (EH-1)
- Use `Action::requeue(duration)` for retryable errors (EH-1)
- Use `Action::await_change()` for terminal states (EH-1)
- One controller per CRD type (CS-1)
- Apply standard labels to all owned resources (CS-2)
- ALWAYS update status conditions using the standard pattern (SC-1)
- MUST set `observedGeneration` in status (SC-1)
- Follow least-privilege RBAC (RB-1)

### ❌ NEVER DO
- Use raw `serde_json::Value` for CRD spec or status fields (CD-1)
- Panic inside reconciler functions (EH-1)
- Grant wildcard (`*`) RBAC permissions (RB-2)
- Create owned resources without standard labels (CS-2)
- Skip finalizers when managing external resources (RP-2)
- Trigger status updates without setting `observedGeneration` (SC-1)
- Run the same operator controller loop for multiple CRD types (CS-1)

---

## Related Documentation

- **Main Quick Reference**: @CLAUDE.md (see Quick Reference section)
- **Main Standards**: @CLAUDE.md
- **Anti-Patterns**: @instructions/ANTI_PATTERNS.md
- **Testing Standards**: @instructions/TESTING_STANDARDS.md
- **kube-rs Documentation**: <https://docs.rs/kube>
- **Kubernetes Operator Pattern**: <https://kubernetes.io/docs/concepts/extend-kubernetes/operator/>
