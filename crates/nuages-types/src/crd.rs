//! CRD type definitions for the Nuages PaaS platform.
//!
//! Defines the `ReinhardtApp` custom resource following the Kubernetes
//! operator pattern with strongly typed spec and status fields.

use std::collections::BTreeMap;

use kube::CustomResource;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::validation::ValidationError;

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

impl ScaleSpec {
	/// Validates the autoscaling specification.
	///
	/// Checks that replica counts are non-negative, max >= min when both
	/// are present, and target_value is positive.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(min) = self.min_replicas
			&& min < 0
		{
			errors.push(ValidationError::new("scale.min_replicas must be >= 0"));
		}

		if let Some(max) = self.max_replicas
			&& max < 0
		{
			errors.push(ValidationError::new("scale.max_replicas must be >= 0"));
		}

		if let (Some(min), Some(max)) = (self.min_replicas, self.max_replicas)
			&& max < min
		{
			errors.push(ValidationError::new(
				"scale.max_replicas must be >= scale.min_replicas",
			));
		}

		if let Some(target) = self.target_value
			&& target <= 0
		{
			errors.push(ValidationError::new("scale.target_value must be > 0"));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
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

impl HealthSpec {
	/// Validates the health check specification.
	///
	/// Checks that port is within the valid range (1-65535) and
	/// interval_seconds is positive.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(port) = self.port
			&& !(1..=65535).contains(&port)
		{
			errors.push(ValidationError::new(
				"health.port must be between 1 and 65535",
			));
		}

		if let Some(interval) = self.interval_seconds
			&& interval <= 0
		{
			errors.push(ValidationError::new("health.interval_seconds must be > 0"));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
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

impl ServicesSpec {
	/// Validates the service exposure specification.
	///
	/// Checks that port and target_port are within the valid range (1-65535).
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(port) = self.port
			&& !(1..=65535).contains(&port)
		{
			errors.push(ValidationError::new(
				"services.port must be between 1 and 65535",
			));
		}

		if let Some(target_port) = self.target_port
			&& !(1..=65535).contains(&target_port)
		{
			errors.push(ValidationError::new(
				"services.target_port must be between 1 and 65535",
			));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

/// Type of a Kubernetes-style status condition.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum ConditionType {
	Ready,
	Progressing,
	Degraded,
}

/// Status value for a Kubernetes-style status condition.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum ConditionStatus {
	True,
	False,
	Unknown,
}

/// Standard Kubernetes-style condition for status reporting.
///
/// Compatible with `k8s_openapi::apimachinery::pkg::apis::meta::v1::Condition`
/// but implements `JsonSchema` for CRD schema generation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppCondition {
	/// Type of the condition
	#[serde(rename = "type")]
	pub type_: ConditionType,
	/// Status of the condition
	pub status: ConditionStatus,
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

impl ReinhardtAppSpec {
	/// Validates the full application specification.
	///
	/// Checks replicas and delegates to nested spec validations,
	/// collecting all errors.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(replicas) = self.replicas
			&& replicas < 0
		{
			errors.push(ValidationError::new("spec.replicas must be >= 0"));
		}

		if let Some(ref scale) = self.scale
			&& let Err(errs) = scale.validate()
		{
			errors.extend(errs);
		}

		if let Some(ref health) = self.health
			&& let Err(errs) = health.validate()
		{
			errors.extend(errs);
		}

		if let Some(ref services) = self.services
			&& let Err(errs) = services.validate()
		{
			errors.extend(errs);
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

/// Status of the `ReinhardtApp` custom resource.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
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
	fn scale_spec_validation_valid() {
		// Arrange
		let spec = ScaleSpec {
			min_replicas: Some(1),
			max_replicas: Some(10),
			metric: Some(ScaleMetric::Cpu),
			target_value: Some(80),
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn scale_spec_validation_negative_replicas() {
		// Arrange
		let spec = ScaleSpec {
			min_replicas: Some(-1),
			max_replicas: Some(10),
			metric: None,
			target_value: None,
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert_eq!(errors[0].message, "scale.min_replicas must be >= 0");
	}

	#[rstest]
	fn scale_spec_validation_max_less_than_min() {
		// Arrange
		let spec = ScaleSpec {
			min_replicas: Some(10),
			max_replicas: Some(5),
			metric: None,
			target_value: None,
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert_eq!(
			errors[0].message,
			"scale.max_replicas must be >= scale.min_replicas"
		);
	}

	#[rstest]
	fn health_spec_validation_invalid_port() {
		// Arrange
		let spec_zero = HealthSpec {
			path: None,
			port: Some(0),
			interval_seconds: None,
		};
		let spec_over = HealthSpec {
			path: None,
			port: Some(65536),
			interval_seconds: None,
		};
		let spec_negative = HealthSpec {
			path: None,
			port: Some(-1),
			interval_seconds: None,
		};

		// Act
		let result_zero = spec_zero.validate();
		let result_over = spec_over.validate();
		let result_negative = spec_negative.validate();

		// Assert
		let errors_zero = result_zero.unwrap_err();
		assert_eq!(errors_zero.len(), 1);
		assert_eq!(
			errors_zero[0].message,
			"health.port must be between 1 and 65535"
		);
		let errors_over = result_over.unwrap_err();
		assert_eq!(errors_over.len(), 1);
		assert_eq!(
			errors_over[0].message,
			"health.port must be between 1 and 65535"
		);
		let errors_negative = result_negative.unwrap_err();
		assert_eq!(errors_negative.len(), 1);
		assert_eq!(
			errors_negative[0].message,
			"health.port must be between 1 and 65535"
		);
	}

	#[rstest]
	fn health_spec_validation_zero_interval() {
		// Arrange
		let spec = HealthSpec {
			path: None,
			port: None,
			interval_seconds: Some(0),
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert_eq!(errors[0].message, "health.interval_seconds must be > 0");
	}

	#[rstest]
	fn services_spec_validation_invalid_ports() {
		// Arrange
		let spec = ServicesSpec {
			port: Some(0),
			target_port: Some(65536),
			ingress_host: None,
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 2);
		assert_eq!(
			errors[0].message,
			"services.port must be between 1 and 65535"
		);
		assert_eq!(
			errors[1].message,
			"services.target_port must be between 1 and 65535"
		);
	}

	#[rstest]
	fn reinhardt_app_spec_validation_collects_all_errors() {
		// Arrange
		let spec = ReinhardtAppSpec {
			image: "myapp:latest".to_string(),
			replicas: Some(-1),
			database: None,
			scale: Some(ScaleSpec {
				min_replicas: Some(-1),
				max_replicas: Some(-2),
				metric: None,
				target_value: Some(0),
			}),
			health: Some(HealthSpec {
				path: None,
				port: Some(0),
				interval_seconds: Some(0),
			}),
			services: Some(ServicesSpec {
				port: Some(0),
				target_port: Some(65536),
				ingress_host: None,
			}),
			env: BTreeMap::new(),
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		// replicas(-1) + min(-1) + max(-2) + max<min + target(0) + health.port(0) + interval(0) + services.port(0) + services.target_port(65536)
		assert_eq!(errors.len(), 9);
		assert_eq!(errors[0].message, "spec.replicas must be >= 0");
		assert_eq!(errors[1].message, "scale.min_replicas must be >= 0");
		assert_eq!(errors[2].message, "scale.max_replicas must be >= 0");
		assert_eq!(
			errors[3].message,
			"scale.max_replicas must be >= scale.min_replicas"
		);
		assert_eq!(errors[4].message, "scale.target_value must be > 0");
		assert_eq!(errors[5].message, "health.port must be between 1 and 65535");
		assert_eq!(errors[6].message, "health.interval_seconds must be > 0");
		assert_eq!(
			errors[7].message,
			"services.port must be between 1 and 65535"
		);
		assert_eq!(
			errors[8].message,
			"services.target_port must be between 1 and 65535"
		);
	}

	#[rstest]
	fn crd_status_with_conditions() {
		// Arrange
		let status = ReinhardtAppStatus {
			phase: Some(AppPhase::Running),
			conditions: vec![AppCondition {
				type_: ConditionType::Ready,
				status: ConditionStatus::True,
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
		assert_eq!(deserialized.conditions[0].type_, ConditionType::Ready);
		assert_eq!(deserialized.conditions[0].status, ConditionStatus::True);
		assert_eq!(deserialized.observed_generation, Some(1));
		assert_eq!(deserialized.ready_replicas, Some(3));
	}

	#[rstest]
	fn condition_enums_serialization_roundtrip() {
		// Arrange
		let types = [
			(ConditionType::Ready, "\"Ready\""),
			(ConditionType::Progressing, "\"Progressing\""),
			(ConditionType::Degraded, "\"Degraded\""),
		];
		let statuses = [
			(ConditionStatus::True, "\"True\""),
			(ConditionStatus::False, "\"False\""),
			(ConditionStatus::Unknown, "\"Unknown\""),
		];

		for (variant, expected) in &types {
			// Act
			let json = serde_json::to_string(variant).expect("serialization should succeed");

			// Assert
			assert_eq!(json, *expected);
		}

		for (variant, expected) in &statuses {
			// Act
			let json = serde_json::to_string(variant).expect("serialization should succeed");

			// Assert
			assert_eq!(json, *expected);
		}
	}

	#[rstest]
	fn status_camelcase_serialization() {
		// Arrange
		let status = ReinhardtAppStatus {
			phase: Some(AppPhase::Running),
			conditions: vec![AppCondition {
				type_: ConditionType::Ready,
				status: ConditionStatus::True,
				reason: "ReconcileSuccess".to_string(),
				message: "All good".to_string(),
				last_transition_time: Some("2025-01-01T00:00:00Z".to_string()),
				observed_generation: Some(1),
			}],
			observed_generation: Some(2),
			ready_replicas: Some(3),
		};

		// Act
		let json = serde_json::to_string(&status).expect("serialization should succeed");
		let value: serde_json::Value = serde_json::from_str(&json).expect("parsing should succeed");

		// Assert
		assert!(value.get("observedGeneration").is_some());
		assert!(value.get("observed_generation").is_none());
		assert!(value.get("readyReplicas").is_some());
		assert!(value.get("ready_replicas").is_none());

		let condition = &value["conditions"][0];
		assert!(condition.get("lastTransitionTime").is_some());
		assert!(condition.get("last_transition_time").is_none());
		assert!(condition.get("observedGeneration").is_some());
		assert!(condition.get("observed_generation").is_none());
		// "type" field keeps explicit rename over rename_all
		assert!(condition.get("type").is_some());
	}
}
