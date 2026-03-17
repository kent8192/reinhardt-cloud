//! Status types for the `ReinhardtApp` custom resource.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::enums::AppPhase;

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
