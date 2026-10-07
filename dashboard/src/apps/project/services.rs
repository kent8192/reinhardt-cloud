//! Immutable runtime identity and serializable project read contracts.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EnvironmentKind {
	Production,
	Staging,
	Preview,
}

impl EnvironmentKind {
	pub fn from_storage(value: &str) -> Option<Self> {
		match value {
			"production" => Some(Self::Production),
			"staging" => Some(Self::Staging),
			"preview" => Some(Self::Preview),
			_ => None,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CpuAutoscaling {
	pub min_replicas: u32,
	pub max_replicas: u32,
	pub target_cpu_percent: u32,
}

/// Persist only desired runtime inputs; cluster observations are separate state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DesiredRuntime {
	pub replicas: u32,
	pub autoscaling: Option<CpuAutoscaling>,
	pub restart_nonce: Option<Uuid>,
}

impl Default for DesiredRuntime {
	fn default() -> Self {
		Self {
			replicas: 1,
			autoscaling: None,
			restart_nonce: None,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentTarget {
	pub organization_id: Uuid,
	pub project_id: Uuid,
	pub environment_id: Uuid,
	pub kind: EnvironmentKind,
}

impl EnvironmentTarget {
	/// Display-name changes never change the namespace identity.
	pub fn namespace(&self) -> String {
		format!("env-{}", self.environment_id.simple())
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSummary {
	pub id: Uuid,
	pub organization_id: Uuid,
	pub name: String,
	pub repository: String,
	pub environments: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvironmentSummary {
	pub id: Uuid,
	pub kind: EnvironmentKind,
	pub version: i64,
	pub desired_runtime: DesiredRuntime,
	pub latest_operation: Option<crate::apps::deployment::services::OperationSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDetail {
	pub id: Uuid,
	pub organization_id: Uuid,
	pub name: String,
	pub repository: String,
	pub environments: Vec<EnvironmentSummary>,
}
