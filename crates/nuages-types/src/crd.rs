//! Kubernetes Custom Resource Definitions for the nuages PaaS platform.

use k8s_openapi::apimachinery::pkg::apis::meta::v1::Condition;
use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Phase of the application lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub enum AppPhase {
	/// Application is being created.
	#[default]
	Pending,
	/// Application is running and healthy.
	Running,
	/// Application has encountered an error.
	Failed,
	/// Application is being deleted.
	Terminating,
}

/// Supported database backends for a `ReinhardtApp`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum DatabaseType {
	/// PostgreSQL database.
	PostgreSQL,
}

/// Metric used for horizontal auto-scaling decisions.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum ScaleMetric {
	/// Scale based on CPU utilisation.
	Cpu,
	/// Scale based on memory utilisation.
	Memory,
}

/// Horizontal scaling configuration for a `ReinhardtApp`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScaleSpec {
	/// Minimum number of replicas.
	pub min_replicas: Option<i32>,
	/// Maximum number of replicas.
	pub max_replicas: Option<i32>,
	/// Metric to use for scaling decisions.
	pub metric: Option<ScaleMetric>,
	/// Target utilisation percentage (0-100).
	pub target_utilization: Option<i32>,
}

/// Health-check configuration for a `ReinhardtApp`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HealthSpec {
	/// HTTP path for liveness probes (e.g. `/healthz`).
	pub liveness_path: Option<String>,
	/// HTTP path for readiness probes (e.g. `/readyz`).
	pub readiness_path: Option<String>,
}

/// Networking / service configuration for a `ReinhardtApp`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServicesSpec {
	/// Kubernetes Service port (defaults to 80).
	pub port: Option<i32>,
	/// Container target port (defaults to 8000).
	pub target_port: Option<i32>,
}

/// Spec for the `ReinhardtApp` custom resource.
#[derive(CustomResource, Debug, Clone, Serialize, Deserialize, JsonSchema)]
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
	/// Container image to deploy (e.g. `myapp:latest`).
	pub image: String,
	/// Desired replica count (defaults to 1).
	pub replicas: Option<i32>,
	/// Optional database backend for the application.
	pub database: Option<DatabaseType>,
	/// Horizontal scaling configuration.
	pub scale: Option<ScaleSpec>,
	/// Health-check probe paths.
	pub health: Option<HealthSpec>,
	/// Networking / service configuration.
	pub services: Option<ServicesSpec>,
}

/// Observed status of a `ReinhardtApp`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct ReinhardtAppStatus {
	/// Standard Kubernetes condition list.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub conditions: Vec<Condition>,
	/// The generation last processed by the controller.
	pub observed_generation: Option<i64>,
	/// Number of ready replicas observed.
	pub ready_replicas: Option<i32>,
	/// Current lifecycle phase.
	pub phase: Option<AppPhase>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_app_phase_default_is_pending() {
		// Arrange / Act
		let phase = AppPhase::default();

		// Assert
		assert_eq!(phase, AppPhase::Pending);
	}

	#[rstest]
	fn test_reinhardt_app_spec_serialization_roundtrip() {
		// Arrange
		let spec = ReinhardtAppSpec {
			image: "myapp:latest".to_string(),
			replicas: Some(3),
			database: Some(DatabaseType::PostgreSQL),
			scale: None,
			health: Some(HealthSpec {
				liveness_path: Some("/healthz".to_string()),
				readiness_path: Some("/readyz".to_string()),
			}),
			services: Some(ServicesSpec {
				port: Some(80),
				target_port: Some(8080),
			}),
		};

		// Act
		let json = serde_json::to_string(&spec).unwrap();
		let deserialized: ReinhardtAppSpec = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.image, "myapp:latest");
		assert_eq!(deserialized.replicas, Some(3));
		assert_eq!(deserialized.database, Some(DatabaseType::PostgreSQL));
		assert!(deserialized.scale.is_none());
		assert_eq!(
			deserialized
				.health
				.as_ref()
				.unwrap()
				.liveness_path
				.as_deref(),
			Some("/healthz")
		);
		assert_eq!(
			deserialized.services.as_ref().unwrap().target_port,
			Some(8080)
		);
	}

	#[rstest]
	fn test_reinhardt_app_status_default_is_empty() {
		// Arrange / Act
		let status = ReinhardtAppStatus::default();

		// Assert
		assert!(status.conditions.is_empty());
		assert!(status.observed_generation.is_none());
		assert!(status.ready_replicas.is_none());
		assert!(status.phase.is_none());
	}
}
