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

impl std::fmt::Display for BuildPhase {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			BuildPhase::Queued => write!(f, "queued"),
			BuildPhase::Pulling => write!(f, "pulling"),
			BuildPhase::Building => write!(f, "building"),
			BuildPhase::Pushing => write!(f, "pushing"),
			BuildPhase::Finalizing => write!(f, "finalizing"),
		}
	}
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

	#[rstest]
	fn test_build_status_completed_with_failure() {
		// Arrange
		let status = BuildStatus {
			build_id: Uuid::new_v4(),
			app_name: "failing-app".to_string(),
			phase: BuildPhase::Finalizing,
			completed: true,
			success: Some(false),
			started_at: Utc::now(),
			completed_at: Some(Utc::now()),
		};

		// Act
		let json = serde_json::to_string(&status).unwrap();
		let deserialized: BuildStatus = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.build_id, status.build_id);
		assert_eq!(deserialized.app_name, "failing-app");
		assert!(deserialized.completed);
		assert_eq!(deserialized.success, Some(false));
		assert!(deserialized.completed_at.is_some());
	}

	#[rstest]
	fn test_build_event_log_unicode_message() {
		// Arrange
		let event = BuildEvent::Log {
			message: "こんにちは 🚀".to_string(),
			timestamp: Utc::now(),
		};

		// Act
		let json = serde_json::to_string(&event).unwrap();
		let deserialized: BuildEvent = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized, event);
		if let BuildEvent::Log { message, .. } = &deserialized {
			assert_eq!(message, "こんにちは 🚀");
		} else {
			panic!("Expected BuildEvent::Log variant");
		}
	}

	#[rstest]
	fn test_env_var_empty_key_value() {
		// Arrange
		let ev = EnvVar {
			key: "".to_string(),
			value: "".to_string(),
		};

		// Act
		let json = serde_json::to_string(&ev).unwrap();
		let deserialized: EnvVar = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.key, "");
		assert_eq!(deserialized.value, "");
	}

	#[rstest]
	fn test_build_request_empty_app_name() {
		// Arrange
		let request = BuildRequest {
			app_name: "".to_string(),
			image: "img:latest".to_string(),
			env_vars: vec![],
			dockerfile: None,
			context_path: None,
		};

		// Act
		let json = serde_json::to_string(&request).unwrap();
		let deserialized: BuildRequest = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.app_name, "");
		assert_eq!(deserialized.image, "img:latest");
		assert!(deserialized.env_vars.is_empty());
	}

	#[rstest]
	fn test_env_var_very_long_value() {
		// Arrange
		let long_value = "x".repeat(10 * 1024); // 10KB string
		let ev = EnvVar {
			key: "BIG_VAR".to_string(),
			value: long_value.clone(),
		};

		// Act
		let json = serde_json::to_string(&ev).unwrap();
		let deserialized: EnvVar = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.key, "BIG_VAR");
		assert_eq!(deserialized.value, long_value);
		assert_eq!(deserialized.value.len(), 10 * 1024);
	}

	#[rstest]
	#[case(None, None)]
	#[case(Some("Dockerfile"), None)]
	#[case(None, Some("."))]
	#[case(Some("Dockerfile.prod"), Some("./app"))]
	fn test_build_request_optional_combinations(
		#[case] dockerfile: Option<&str>,
		#[case] context_path: Option<&str>,
	) {
		// Arrange
		let request = BuildRequest {
			app_name: "combo-app".to_string(),
			image: "img:v1".to_string(),
			env_vars: vec![],
			dockerfile: dockerfile.map(String::from),
			context_path: context_path.map(String::from),
		};

		// Act
		let json = serde_json::to_string(&request).unwrap();
		let deserialized: BuildRequest = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.dockerfile, dockerfile.map(String::from));
		assert_eq!(deserialized.context_path, context_path.map(String::from));
	}

	#[rstest]
	fn test_build_event_debug_impl() {
		// Arrange
		let now = Utc::now();
		let variants: Vec<(&str, BuildEvent)> = vec![
			(
				"Log",
				BuildEvent::Log {
					message: "msg".to_string(),
					timestamp: now,
				},
			),
			(
				"PhaseChange",
				BuildEvent::PhaseChange {
					phase: BuildPhase::Queued,
					timestamp: now,
				},
			),
			(
				"ArtifactReady",
				BuildEvent::ArtifactReady {
					artifact_url: "url".to_string(),
					digest: "d".to_string(),
					timestamp: now,
				},
			),
			(
				"Error",
				BuildEvent::Error {
					message: "err".to_string(),
					timestamp: now,
				},
			),
			(
				"Complete",
				BuildEvent::Complete {
					success: true,
					timestamp: now,
				},
			),
		];

		// Act & Assert
		for (name, variant) in &variants {
			let debug_str = format!("{:?}", variant);
			assert!(!debug_str.is_empty());
			assert!(
				debug_str.contains(name),
				"Debug output for {} should contain variant name, got: {}",
				name,
				debug_str
			);
		}
	}

	#[rstest]
	fn test_build_event_clone_all_variants() {
		// Arrange
		let now = Utc::now();
		let variants = vec![
			BuildEvent::Log {
				message: "clone test".to_string(),
				timestamp: now,
			},
			BuildEvent::PhaseChange {
				phase: BuildPhase::Building,
				timestamp: now,
			},
			BuildEvent::ArtifactReady {
				artifact_url: "url".to_string(),
				digest: "sha256:abc".to_string(),
				timestamp: now,
			},
			BuildEvent::Error {
				message: "err".to_string(),
				timestamp: now,
			},
			BuildEvent::Complete {
				success: false,
				timestamp: now,
			},
		];

		// Act & Assert
		for variant in &variants {
			let cloned = variant.clone();
			assert_eq!(&cloned, variant);
		}
	}

	mod proptest_build {
		use super::*;
		use proptest::prelude::*;

		proptest! {
			#[test]
			fn prop_build_phase_serde_roundtrip(phase_idx in 0..5u8) {
				let phase = match phase_idx {
					0 => BuildPhase::Queued,
					1 => BuildPhase::Pulling,
					2 => BuildPhase::Building,
					3 => BuildPhase::Pushing,
					_ => BuildPhase::Finalizing,
				};
				let json = serde_json::to_string(&phase).unwrap();
				let deserialized: BuildPhase = serde_json::from_str(&json).unwrap();
				prop_assert_eq!(deserialized, phase);
			}

			#[test]
			fn prop_env_var_serde_roundtrip(key in "\\PC*", value in "\\PC*") {
				let ev = EnvVar { key: key.clone(), value: value.clone() };
				let json = serde_json::to_string(&ev).unwrap();
				let deserialized: EnvVar = serde_json::from_str(&json).unwrap();
				prop_assert_eq!(deserialized.key, key);
				prop_assert_eq!(deserialized.value, value);
			}

			#[test]
			fn fuzz_build_event_deserialize_no_panic(s in "\\PC*") {
				// Should either parse or return Err, never panic
				let _ = serde_json::from_str::<BuildEvent>(&s);
			}
		}
	}
}
