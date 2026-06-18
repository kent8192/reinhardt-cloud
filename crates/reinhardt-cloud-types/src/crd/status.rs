//! Status types for the `Project` custom resource.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::cache::CacheStatus;
use super::database::DatabaseStatus;
use super::enums::ProjectPhase;
use super::worker::WorkerStatus;

/// Type of a Kubernetes-style status condition.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum ConditionType {
	Ready,
	Progressing,
	Degraded,
	/// Database migration for the active deployment revision has completed.
	MigrationReady,
	/// Database sub-resource is provisioned and reachable
	DatabaseReady,
	/// Cache sub-resource is provisioned and reachable
	CacheReady,
	/// Worker deployment is running and healthy
	WorkerReady,
	/// Ingress resource is configured and healthy
	IngressReady,
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
pub struct ProjectCondition {
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

/// Status of a single preview environment, aggregated on the parent `Project`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreviewStatus {
	/// Preview Project name, e.g. `my-app-pr-42`.
	pub name: String,
	/// Pull/merge request number.
	pub pr_number: String,
	/// Resolved preview URL, e.g. `https://my-app-pr-42.preview.example.com`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub url: Option<String>,
	/// Current phase reported by the preview Project.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub phase: Option<ProjectPhase>,
	/// Ready replicas of the preview Project.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub ready_replicas: Option<i32>,
	/// Last activity timestamp (RFC 3339), mirrors the TTL annotation.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub last_activity: Option<String>,
}

/// Status of the `Project` custom resource.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatus {
	/// Current phase of the application
	pub phase: Option<ProjectPhase>,
	/// Standard Kubernetes condition list
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub conditions: Vec<ProjectCondition>,
	/// The generation last observed by the controller
	pub observed_generation: Option<i64>,
	/// Number of ready replicas
	pub ready_replicas: Option<i32>,
	/// Status of the provisioned database sub-resource
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub database: Option<DatabaseStatus>,
	/// Status of the provisioned cache sub-resource
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub cache: Option<CacheStatus>,
	/// Status of the worker deployment sub-resource
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub worker: Option<WorkerStatus>,
	/// Preview environments aggregated from child preview Projects.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub previews: Vec<PreviewStatus>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn crd_status_with_conditions() {
		// Arrange
		let status = ProjectStatus {
			phase: Some(ProjectPhase::Running),
			conditions: vec![ProjectCondition {
				type_: ConditionType::Ready,
				status: ConditionStatus::True,
				reason: "ReconcileSuccess".to_string(),
				message: "Application is ready".to_string(),
				last_transition_time: Some("2025-01-01T00:00:00Z".to_string()),
				observed_generation: Some(1),
			}],
			observed_generation: Some(1),
			ready_replicas: Some(3),
			..Default::default()
		};

		// Act
		let json = serde_json::to_string(&status).expect("serialization should succeed");
		let deserialized: ProjectStatus =
			serde_json::from_str(&json).expect("deserialization should succeed");

		// Assert
		assert_eq!(deserialized.phase, Some(ProjectPhase::Running));
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
			(ConditionType::MigrationReady, "\"MigrationReady\""),
			(ConditionType::DatabaseReady, "\"DatabaseReady\""),
			(ConditionType::CacheReady, "\"CacheReady\""),
			(ConditionType::WorkerReady, "\"WorkerReady\""),
			(ConditionType::IngressReady, "\"IngressReady\""),
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
	fn test_status_with_database_status() {
		// Arrange
		use super::super::database::ResourcePhase;
		let status = ProjectStatus {
			phase: Some(ProjectPhase::Running),
			database: Some(DatabaseStatus {
				phase: ResourcePhase::Ready,
				endpoint: Some("mydb.rds.amazonaws.com:5432".to_string()),
				credentials_secret: Some("myapp-db-credentials".to_string()),
			}),
			..Default::default()
		};

		// Act
		let json = serde_json::to_string(&status).unwrap();
		let parsed: ProjectStatus = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(parsed.database.unwrap().phase, ResourcePhase::Ready);
	}

	#[rstest]
	fn status_camelcase_serialization() {
		// Arrange
		let status = ProjectStatus {
			phase: Some(ProjectPhase::Running),
			conditions: vec![ProjectCondition {
				type_: ConditionType::Ready,
				status: ConditionStatus::True,
				reason: "ReconcileSuccess".to_string(),
				message: "All good".to_string(),
				last_transition_time: Some("2025-01-01T00:00:00Z".to_string()),
				observed_generation: Some(1),
			}],
			observed_generation: Some(2),
			ready_replicas: Some(3),
			..Default::default()
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

	#[rstest]
	fn project_status_with_previews_roundtrip() {
		// Arrange
		let status = ProjectStatus {
			previews: vec![PreviewStatus {
				name: "my-app-pr-42".to_string(),
				pr_number: "42".to_string(),
				url: Some("https://my-app-pr-42.preview.example.com".to_string()),
				phase: Some(ProjectPhase::Running),
				ready_replicas: Some(1),
				last_activity: Some("2026-06-18T00:00:00Z".to_string()),
			}],
			..Default::default()
		};

		// Act
		let json = serde_json::to_string(&status).unwrap();
		let back: ProjectStatus = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(back.previews.len(), 1);
		assert_eq!(back.previews[0].pr_number, "42");
		assert_eq!(
			back.previews[0].url.as_deref(),
			Some("https://my-app-pr-42.preview.example.com")
		);
		// Casing: nested preview fields must be camelCase, consistent with ProjectStatus.
		assert!(
			json.contains("\"prNumber\""),
			"pr_number must serialize as prNumber"
		);
		assert!(
			json.contains("\"readyReplicas\""),
			"ready_replicas must serialize as readyReplicas"
		);
		assert!(
			json.contains("\"lastActivity\""),
			"last_activity must serialize as lastActivity"
		);
		assert!(
			!json.contains("pr_number"),
			"snake_case pr_number must not appear"
		);
	}

	#[rstest]
	fn empty_previews_is_skipped_in_json() {
		// Arrange
		let status = ProjectStatus::default();

		// Act
		let json = serde_json::to_string(&status).unwrap();

		// Assert
		assert!(!json.contains("previews"), "empty previews must be omitted");
	}
}
