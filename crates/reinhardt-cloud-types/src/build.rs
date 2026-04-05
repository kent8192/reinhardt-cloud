//! Build pipeline domain types for gRPC build services.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Request to start a new build.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildRequest {
	/// Application name to build.
	pub app_name: String,
	/// Container image reference (e.g. "registry.example.com/app:latest").
	pub image: String,
	/// Environment variables for the build context.
	pub env_vars: Vec<EnvVar>,
	/// Optional Dockerfile path relative to the build context.
	pub dockerfile: Option<String>,
	/// Optional build context path.
	pub context_path: Option<String>,
}

/// A key-value environment variable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnvVar {
	pub key: String,
	pub value: String,
}

/// Events emitted during a build process (streamed to clients).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BuildEvent {
	/// A line of build output.
	Log {
		message: String,
		timestamp: DateTime<Utc>,
	},
	/// The build has transitioned to a new phase.
	PhaseChange {
		phase: BuildPhase,
		timestamp: DateTime<Utc>,
	},
	/// A build artifact is ready for consumption.
	ArtifactReady {
		artifact_url: String,
		digest: String,
		timestamp: DateTime<Utc>,
	},
	/// An error occurred during the build.
	Error {
		message: String,
		timestamp: DateTime<Utc>,
	},
	/// The build has completed.
	Complete {
		success: bool,
		timestamp: DateTime<Utc>,
	},
}

/// Build pipeline phases.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BuildPhase {
	Queued,
	Pulling,
	Building,
	Pushing,
	Finalizing,
}

/// Current status of a build.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildStatus {
	/// Unique build identifier.
	pub build_id: Uuid,
	/// Application name.
	pub app_name: String,
	/// Current build phase.
	pub phase: BuildPhase,
	/// Whether the build has completed.
	pub completed: bool,
	/// Whether the build succeeded (None if still running).
	pub success: Option<bool>,
	/// When the build was started.
	pub started_at: DateTime<Utc>,
	/// When the build completed (None if still running).
	pub completed_at: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_build_request_serde_roundtrip() {
		// Arrange
		let request = BuildRequest {
			app_name: "my-app".to_string(),
			image: "registry.example.com/my-app:v1".to_string(),
			env_vars: vec![EnvVar {
				key: "NODE_ENV".to_string(),
				value: "production".to_string(),
			}],
			dockerfile: Some("Dockerfile.prod".to_string()),
			context_path: None,
		};

		// Act
		let json = serde_json::to_string(&request).unwrap();
		let deserialized: BuildRequest = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.app_name, "my-app");
		assert_eq!(deserialized.env_vars.len(), 1);
		assert_eq!(deserialized.env_vars[0].key, "NODE_ENV");
		assert_eq!(deserialized.dockerfile, Some("Dockerfile.prod".to_string()));
		assert!(deserialized.context_path.is_none());
	}

	#[rstest]
	fn test_build_event_variants_serde_roundtrip() {
		// Arrange
		let now = Utc::now();
		let events = vec![
			BuildEvent::Log {
				message: "Step 1/5: FROM rust:1.80".to_string(),
				timestamp: now,
			},
			BuildEvent::PhaseChange {
				phase: BuildPhase::Building,
				timestamp: now,
			},
			BuildEvent::ArtifactReady {
				artifact_url: "registry.example.com/app:abc123".to_string(),
				digest: "sha256:deadbeef".to_string(),
				timestamp: now,
			},
			BuildEvent::Error {
				message: "compilation failed".to_string(),
				timestamp: now,
			},
			BuildEvent::Complete {
				success: true,
				timestamp: now,
			},
		];

		// Act & Assert
		for event in &events {
			let json = serde_json::to_string(event).unwrap();
			let deserialized: BuildEvent = serde_json::from_str(&json).unwrap();
			assert_eq!(&deserialized, event);
		}
	}

	#[rstest]
	fn test_build_status_serde_roundtrip() {
		// Arrange
		let status = BuildStatus {
			build_id: Uuid::new_v4(),
			app_name: "test-app".to_string(),
			phase: BuildPhase::Pushing,
			completed: false,
			success: None,
			started_at: Utc::now(),
			completed_at: None,
		};

		// Act
		let json = serde_json::to_string(&status).unwrap();
		let deserialized: BuildStatus = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.build_id, status.build_id);
		assert_eq!(deserialized.app_name, "test-app");
		assert!(!deserialized.completed);
		assert!(deserialized.success.is_none());
	}

	#[rstest]
	fn test_build_phase_all_variants() {
		// Arrange
		let phases = vec![
			BuildPhase::Queued,
			BuildPhase::Pulling,
			BuildPhase::Building,
			BuildPhase::Pushing,
			BuildPhase::Finalizing,
		];

		// Act & Assert
		for phase in &phases {
			let json = serde_json::to_string(phase).unwrap();
			let deserialized: BuildPhase = serde_json::from_str(&json).unwrap();
			assert_eq!(&deserialized, phase);
		}
	}
}
