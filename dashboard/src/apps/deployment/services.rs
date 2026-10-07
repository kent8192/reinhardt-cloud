//! Operation state transitions and fencing of uncertain execution.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::apps::project::services::{CpuAutoscaling, DesiredRuntime};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RuntimeChange {
	Restart,
	Scale { replicas: u32 },
	Autoscale { configuration: CpuAutoscaling },
}

impl RuntimeChange {
	pub fn storage_name(&self) -> &'static str {
		match self {
			Self::Restart => "restart",
			Self::Scale { .. } => "scale",
			Self::Autoscale { .. } => "autoscale",
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationRequest {
	pub organization_id: Uuid,
	pub environment_id: Uuid,
	pub expected_version: i64,
	pub idempotency_key: String,
	pub change: RuntimeChange,
}

/// The same immutable envelope is delivered again until its correlated receipt arrives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchEnvelope {
	pub schema_version: u32,
	pub operation_id: Uuid,
	pub organization_id: Uuid,
	pub environment_id: Uuid,
	pub environment_version: i64,
	pub cluster_id: Uuid,
	pub change: RuntimeChange,
	pub desired_runtime: DesiredRuntime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationState {
	Queued,
	Building,
	Migrating,
	Applying,
	Verifying,
	Uncertain,
	Succeeded,
	Failed,
	Cancelled,
}

impl OperationState {
	pub fn storage_name(self) -> &'static str {
		match self {
			Self::Queued => "queued",
			Self::Building => "building",
			Self::Migrating => "migrating",
			Self::Applying => "applying",
			Self::Verifying => "verifying",
			Self::Uncertain => "uncertain",
			Self::Succeeded => "succeeded",
			Self::Failed => "failed",
			Self::Cancelled => "cancelled",
		}
	}

	pub fn from_storage(value: &str) -> Option<Self> {
		match value {
			"queued" => Some(Self::Queued),
			"building" => Some(Self::Building),
			"migrating" => Some(Self::Migrating),
			"applying" => Some(Self::Applying),
			"verifying" => Some(Self::Verifying),
			"uncertain" => Some(Self::Uncertain),
			"succeeded" => Some(Self::Succeeded),
			"failed" => Some(Self::Failed),
			"cancelled" => Some(Self::Cancelled),
			_ => None,
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationError {
	InvalidTransition,
	UnsafeCancellation,
	StaleResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationProgress {
	pub id: Uuid,
	pub environment_id: Uuid,
	pub environment_version: i64,
	pub state: OperationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationSnapshot {
	pub progress: OperationProgress,
	pub kind: String,
	pub created_at: chrono::DateTime<chrono::Utc>,
}

impl OperationProgress {
	pub fn transition(&mut self, next: OperationState) -> Result<(), OperationError> {
		use OperationState::{
			Applying, Building, Cancelled, Failed, Migrating, Queued, Succeeded, Uncertain,
			Verifying,
		};
		if self.state == next {
			return Ok(());
		}
		let allowed = matches!(
			(self.state, next),
			(Queued, Building | Applying | Cancelled | Failed | Uncertain)
				| (
					Building,
					Migrating | Applying | Failed | Cancelled | Uncertain
				) | (Migrating, Applying | Failed | Uncertain)
				| (Applying, Verifying | Failed | Uncertain)
				| (Verifying, Succeeded | Failed | Uncertain)
				| (
					Uncertain,
					Building | Migrating | Applying | Verifying | Succeeded | Failed
				)
		);
		if !allowed {
			return Err(OperationError::InvalidTransition);
		}
		self.state = next;
		Ok(())
	}

	pub fn cancel(&mut self) -> Result<(), OperationError> {
		if !matches!(
			self.state,
			OperationState::Queued | OperationState::Building
		) {
			return Err(OperationError::UnsafeCancellation);
		}
		self.transition(OperationState::Cancelled)
	}

	/// A deadline does not establish that cluster execution has stopped.
	pub fn deadline_exceeded(&mut self) -> Result<(), OperationError> {
		self.transition(OperationState::Uncertain)
	}

	pub fn reconcile(
		&mut self,
		operation_id: Uuid,
		environment_version: i64,
		next: OperationState,
	) -> Result<(), OperationError> {
		if self.id != operation_id || self.environment_version != environment_version {
			return Err(OperationError::StaleResult);
		}
		self.transition(next)
	}

	pub fn blocks_environment(&self) -> bool {
		!matches!(
			self.state,
			OperationState::Succeeded | OperationState::Failed | OperationState::Cancelled
		)
	}

	pub fn restart_nonce(&self) -> String {
		self.id.simple().to_string()
	}
}
