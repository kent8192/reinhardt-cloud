//! CRD type definitions for the Nuages PaaS platform.
//!
//! Defines the `ReinhardtApp` custom resource following the Kubernetes
//! operator pattern with strongly typed spec and status fields.

use std::collections::BTreeMap;

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Phase of the `ReinhardtApp` lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum AppPhase {
	Pending,
	Deploying,
	Running,
	Failed,
	Terminating,
}

/// Database backend type.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum DatabaseType {
	PostgreSQL,
	MySQL,
}

/// Metric type for autoscaling.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum ScaleMetric {
	Cpu,
	Memory,
	Rps,
}

/// Autoscaling configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ScaleSpec {
	/// Minimum number of replicas
	pub min_replicas: Option<i32>,
	/// Maximum number of replicas
	pub max_replicas: Option<i32>,
	/// Metric to scale on
	pub metric: Option<ScaleMetric>,
	/// Target value for the scaling metric
	pub target_value: Option<i32>,
}

/// Health check configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct HealthSpec {
	/// HTTP path for health checks
	pub path: Option<String>,
	/// Port for health checks
	pub port: Option<i32>,
	/// Interval between health checks in seconds
	pub interval_seconds: Option<i32>,
}

/// Service exposure configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct ServicesSpec {
	/// Service port
	pub port: Option<i32>,
	/// Target port on the container
	pub target_port: Option<i32>,
	/// Ingress hostname for external access
	pub ingress_host: Option<String>,
}

/// Standard Kubernetes-style condition for status reporting.
///
/// Compatible with `k8s_openapi::apimachinery::pkg::apis::meta::v1::Condition`
/// but implements `JsonSchema` for CRD schema generation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AppCondition {
	/// Type of the condition (e.g., "Ready", "Progressing", "Degraded")
	#[serde(rename = "type")]
	pub type_: String,
	/// Status of the condition: "True", "False", or "Unknown"
	pub status: String,
	/// Machine-readable reason for the condition
	pub reason: String,
	/// Human-readable message
	pub message: String,
	/// Last time the condition transitioned (RFC 3339 format)
	pub last_transition_time: Option<String>,
	/// The generation observed when this condition was set
	pub observed_generation: Option<i64>,
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
	printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#,
	printcolumn = r#"{"name":"Ready","type":"string","jsonPath":".status.conditions[?(@.type==\"Ready\")].status"}"#
)]
pub struct ReinhardtAppSpec {
	/// Docker image to deploy
	pub image: String,
	/// Number of desired replicas (defaults to 1)
	pub replicas: Option<i32>,
	/// Database backend type
	pub database: Option<DatabaseType>,
	/// Autoscaling configuration
	pub scale: Option<ScaleSpec>,
	/// Health check configuration
	pub health: Option<HealthSpec>,
	/// Service exposure configuration
	pub services: Option<ServicesSpec>,
	/// Environment variables as key-value pairs
	#[serde(default)]
	pub env: BTreeMap<String, String>,
}

/// Status of the `ReinhardtApp` custom resource.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct ReinhardtAppStatus {
	/// Current phase of the application
	pub phase: Option<AppPhase>,
	/// Standard Kubernetes condition list
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub conditions: Vec<AppCondition>,
	/// The generation last observed by the controller
	pub observed_generation: Option<i64>,
	/// Number of ready replicas
	pub ready_replicas: Option<i32>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn crd_spec_serialization_roundtrip() {
		// Arrange
		let spec = ReinhardtAppSpec {
			image: "myapp:latest".to_string(),
			replicas: Some(3),
			database: Some(DatabaseType::PostgreSQL),
			scale: Some(ScaleSpec {
				min_replicas: Some(1),
				max_replicas: Some(10),
				metric: Some(ScaleMetric::Cpu),
				target_value: Some(80),
			}),
			health: Some(HealthSpec {
				path: Some("/healthz".to_string()),
				port: Some(8080),
				interval_seconds: Some(30),
			}),
			services: Some(ServicesSpec {
				port: Some(80),
				target_port: Some(8080),
				ingress_host: Some("myapp.example.com".to_string()),
			}),
			env: BTreeMap::from([
				("RUST_LOG".to_string(), "info".to_string()),
				(
					"DATABASE_URL".to_string(),
					"postgres://localhost/db".to_string(),
				),
			]),
		};

		// Act
		let json = serde_json::to_string(&spec).expect("serialization should succeed");
		let deserialized: ReinhardtAppSpec =
			serde_json::from_str(&json).expect("deserialization should succeed");

		// Assert
		assert_eq!(deserialized.image, "myapp:latest");
		assert_eq!(deserialized.replicas, Some(3));
		assert_eq!(deserialized.database, Some(DatabaseType::PostgreSQL));
		assert_eq!(deserialized.env.len(), 2);
		assert_eq!(deserialized.env.get("RUST_LOG").unwrap(), "info");
	}

	#[rstest]
	fn crd_spec_defaults() {
		// Arrange
		let json = r#"{"image": "myapp:v1"}"#;

		// Act
		let spec: ReinhardtAppSpec =
			serde_json::from_str(json).expect("deserialization should succeed");

		// Assert
		assert_eq!(spec.image, "myapp:v1");
		assert_eq!(spec.replicas, None);
		assert_eq!(spec.database, None);
		assert_eq!(spec.scale, None);
		assert_eq!(spec.health, None);
		assert_eq!(spec.services, None);
		assert!(spec.env.is_empty());
	}

	#[rstest]
	fn crd_status_with_conditions() {
		// Arrange
		let status = ReinhardtAppStatus {
			phase: Some(AppPhase::Running),
			conditions: vec![AppCondition {
				type_: "Ready".to_string(),
				status: "True".to_string(),
				reason: "ReconcileSuccess".to_string(),
				message: "Application is ready".to_string(),
				last_transition_time: Some("2025-01-01T00:00:00Z".to_string()),
				observed_generation: Some(1),
			}],
			observed_generation: Some(1),
			ready_replicas: Some(3),
		};

		// Act
		let json = serde_json::to_string(&status).expect("serialization should succeed");
		let deserialized: ReinhardtAppStatus =
			serde_json::from_str(&json).expect("deserialization should succeed");

		// Assert
		assert_eq!(deserialized.phase, Some(AppPhase::Running));
		assert_eq!(deserialized.conditions.len(), 1);
		assert_eq!(deserialized.conditions[0].type_, "Ready");
		assert_eq!(deserialized.conditions[0].status, "True");
		assert_eq!(deserialized.observed_generation, Some(1));
		assert_eq!(deserialized.ready_replicas, Some(3));
	}
}
