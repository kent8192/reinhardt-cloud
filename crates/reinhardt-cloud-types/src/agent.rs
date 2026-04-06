//! Cluster agent domain types for gRPC agent communication.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Information about a connected cluster agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
	/// Unique agent identifier.
	pub agent_id: Uuid,
	/// Cluster name the agent belongs to.
	pub cluster_name: String,
	/// Kubernetes node name where the agent runs.
	pub node_name: String,
	/// Agent software version.
	pub version: String,
	/// When the agent last reported.
	pub last_seen: DateTime<Utc>,
}

/// Commands sent from the control plane to a cluster agent.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentCommand {
	/// Deploy an application to the cluster.
	Deploy {
		app_name: String,
		image: String,
		replicas: u32,
	},
	/// Rollback an application to a previous revision.
	Rollback {
		app_name: String,
		revision: u32,
	},
	/// Scale an application to the specified number of replicas.
	Scale {
		app_name: String,
		replicas: u32,
	},
	/// Restart all pods for an application.
	Restart { app_name: String },
}

/// Events reported by a cluster agent to the control plane.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentEvent {
	/// The agent has connected and is ready.
	Connected {
		agent_id: Uuid,
		cluster_name: String,
		timestamp: DateTime<Utc>,
	},
	/// A deployment operation has completed.
	DeployStatus {
		app_name: String,
		success: bool,
		message: String,
		timestamp: DateTime<Utc>,
	},
	/// A periodic heartbeat.
	Heartbeat {
		agent_id: Uuid,
		timestamp: DateTime<Utc>,
	},
	/// An error occurred on the agent.
	Error {
		message: String,
		timestamp: DateTime<Utc>,
	},
}

/// Health status of a cluster agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentHealth {
	/// Agent identifier.
	pub agent_id: Uuid,
	/// Whether the agent is healthy.
	pub healthy: bool,
	/// CPU usage as a percentage (0.0–100.0).
	pub cpu_usage_percent: f64,
	/// Memory usage as a percentage (0.0–100.0).
	pub memory_usage_percent: f64,
	/// Number of pods managed by this agent.
	pub pod_count: u32,
	/// When this health report was generated.
	pub reported_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_agent_info_serde_roundtrip() {
		// Arrange
		let info = AgentInfo {
			agent_id: Uuid::new_v4(),
			cluster_name: "prod-us-east".to_string(),
			node_name: "node-01".to_string(),
			version: "0.1.0".to_string(),
			last_seen: Utc::now(),
		};

		// Act
		let json = serde_json::to_string(&info).unwrap();
		let deserialized: AgentInfo = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.agent_id, info.agent_id);
		assert_eq!(deserialized.cluster_name, "prod-us-east");
		assert_eq!(deserialized.node_name, "node-01");
	}

	#[rstest]
	fn test_agent_command_variants_serde_roundtrip() {
		// Arrange
		let commands = vec![
			AgentCommand::Deploy {
				app_name: "web".to_string(),
				image: "web:v2".to_string(),
				replicas: 3,
			},
			AgentCommand::Rollback {
				app_name: "web".to_string(),
				revision: 5,
			},
			AgentCommand::Scale {
				app_name: "web".to_string(),
				replicas: 10,
			},
			AgentCommand::Restart {
				app_name: "web".to_string(),
			},
		];

		// Act & Assert
		for cmd in &commands {
			let json = serde_json::to_string(cmd).unwrap();
			let deserialized: AgentCommand = serde_json::from_str(&json).unwrap();
			assert_eq!(&deserialized, cmd);
		}
	}

	#[rstest]
	fn test_agent_event_variants_serde_roundtrip() {
		// Arrange
		let now = Utc::now();
		let agent_id = Uuid::new_v4();
		let events = vec![
			AgentEvent::Connected {
				agent_id,
				cluster_name: "staging".to_string(),
				timestamp: now,
			},
			AgentEvent::DeployStatus {
				app_name: "api".to_string(),
				success: true,
				message: "deployed successfully".to_string(),
				timestamp: now,
			},
			AgentEvent::Heartbeat {
				agent_id,
				timestamp: now,
			},
			AgentEvent::Error {
				message: "connection lost".to_string(),
				timestamp: now,
			},
		];

		// Act & Assert
		for event in &events {
			let json = serde_json::to_string(event).unwrap();
			let deserialized: AgentEvent = serde_json::from_str(&json).unwrap();
			assert_eq!(&deserialized, event);
		}
	}

	#[rstest]
	fn test_agent_health_serde_roundtrip() {
		// Arrange
		let health = AgentHealth {
			agent_id: Uuid::new_v4(),
			healthy: true,
			cpu_usage_percent: 45.2,
			memory_usage_percent: 67.8,
			pod_count: 42,
			reported_at: Utc::now(),
		};

		// Act
		let json = serde_json::to_string(&health).unwrap();
		let deserialized: AgentHealth = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.agent_id, health.agent_id);
		assert!(deserialized.healthy);
		assert!((deserialized.cpu_usage_percent - 45.2).abs() < f64::EPSILON);
		assert_eq!(deserialized.pod_count, 42);
	}

	#[rstest]
	fn test_agent_health_cpu_zero() {
		// Arrange
		let health = AgentHealth {
			agent_id: Uuid::new_v4(),
			healthy: true,
			cpu_usage_percent: 0.0,
			memory_usage_percent: 50.0,
			pod_count: 1,
			reported_at: Utc::now(),
		};

		// Act
		let json = serde_json::to_string(&health).unwrap();
		let deserialized: AgentHealth = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.cpu_usage_percent, 0.0);
	}

	#[rstest]
	fn test_agent_health_cpu_hundred() {
		// Arrange
		let health = AgentHealth {
			agent_id: Uuid::new_v4(),
			healthy: false,
			cpu_usage_percent: 100.0,
			memory_usage_percent: 99.0,
			pod_count: 50,
			reported_at: Utc::now(),
		};

		// Act
		let json = serde_json::to_string(&health).unwrap();
		let deserialized: AgentHealth = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.cpu_usage_percent, 100.0);
		assert!(!deserialized.healthy);
	}

	#[rstest]
	fn test_agent_command_deploy_zero_replicas() {
		// Arrange
		let cmd = AgentCommand::Deploy {
			app_name: "zero-app".to_string(),
			image: "img:v1".to_string(),
			replicas: 0,
		};

		// Act
		let json = serde_json::to_string(&cmd).unwrap();
		let deserialized: AgentCommand = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized, cmd);
		if let AgentCommand::Deploy { replicas, .. } = deserialized {
			assert_eq!(replicas, 0);
		} else {
			panic!("Expected AgentCommand::Deploy variant");
		}
	}

	#[rstest]
	fn test_agent_command_rollback_revision_zero() {
		// Arrange
		let cmd = AgentCommand::Rollback {
			app_name: "rollback-app".to_string(),
			revision: 0,
		};

		// Act
		let json = serde_json::to_string(&cmd).unwrap();
		let deserialized: AgentCommand = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized, cmd);
		if let AgentCommand::Rollback { revision, .. } = deserialized {
			assert_eq!(revision, 0);
		} else {
			panic!("Expected AgentCommand::Rollback variant");
		}
	}

	#[rstest]
	fn test_agent_info_empty_version() {
		// Arrange
		let info = AgentInfo {
			agent_id: Uuid::new_v4(),
			cluster_name: "test-cluster".to_string(),
			node_name: "node-01".to_string(),
			version: "".to_string(),
			last_seen: Utc::now(),
		};

		// Act
		let json = serde_json::to_string(&info).unwrap();
		let deserialized: AgentInfo = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.version, "");
		assert_eq!(deserialized.agent_id, info.agent_id);
	}

	#[rstest]
	#[case(0.0)]
	#[case(0.001)]
	#[case(50.0)]
	#[case(99.999)]
	#[case(100.0)]
	fn test_agent_health_percentage_boundaries(#[case] cpu: f64) {
		// Arrange
		let health = AgentHealth {
			agent_id: Uuid::new_v4(),
			healthy: true,
			cpu_usage_percent: cpu,
			memory_usage_percent: 50.0,
			pod_count: 10,
			reported_at: Utc::now(),
		};

		// Act
		let json = serde_json::to_string(&health).unwrap();
		let deserialized: AgentHealth = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized.cpu_usage_percent, cpu);
	}

	#[rstest]
	#[case(0u32)]
	#[case(1)]
	#[case(u32::MAX)]
	fn test_agent_command_replicas_boundary(#[case] replicas: u32) {
		// Arrange
		let cmd = AgentCommand::Scale {
			app_name: "scale-app".to_string(),
			replicas,
		};

		// Act
		let json = serde_json::to_string(&cmd).unwrap();
		let deserialized: AgentCommand = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized, cmd);
		if let AgentCommand::Scale {
			replicas: deser_replicas,
			..
		} = deserialized
		{
			assert_eq!(deser_replicas, replicas);
		} else {
			panic!("Expected AgentCommand::Scale variant");
		}
	}

	#[rstest]
	fn test_agent_command_debug_impl() {
		// Arrange
		let variants: Vec<(&str, AgentCommand)> = vec![
			(
				"Deploy",
				AgentCommand::Deploy {
					app_name: "app".to_string(),
					image: "img:v1".to_string(),
					replicas: 1,
				},
			),
			(
				"Rollback",
				AgentCommand::Rollback {
					app_name: "app".to_string(),
					revision: 1,
				},
			),
			(
				"Scale",
				AgentCommand::Scale {
					app_name: "app".to_string(),
					replicas: 2,
				},
			),
			(
				"Restart",
				AgentCommand::Restart {
					app_name: "app".to_string(),
				},
			),
		];

		// Act & Assert
		for (name, variant) in &variants {
			let debug_str = format!("{:?}", variant);
			assert!(
				debug_str.contains(name),
				"Debug output for {} should contain variant name, got: {}",
				name,
				debug_str
			);
		}
	}

	mod proptest_agent {
		use super::*;
		use proptest::prelude::*;

		proptest! {
			#[test]
			fn prop_agent_command_serde_roundtrip(
				variant in 0..4u8,
				app_name in "[a-z][a-z0-9-]{0,20}",
				replicas in 0..u32::MAX,
				revision in 0..u32::MAX,
			) {
				let cmd = match variant {
					0 => AgentCommand::Deploy { app_name: app_name.clone(), image: "img:v1".into(), replicas },
					1 => AgentCommand::Rollback { app_name: app_name.clone(), revision },
					2 => AgentCommand::Scale { app_name: app_name.clone(), replicas },
					_ => AgentCommand::Restart { app_name: app_name.clone() },
				};
				let json = serde_json::to_string(&cmd).unwrap();
				let deserialized: AgentCommand = serde_json::from_str(&json).unwrap();
				prop_assert_eq!(deserialized, cmd);
			}

			#[test]
			fn fuzz_agent_command_deserialize_no_panic(s in "\\PC*") {
				let _ = serde_json::from_str::<AgentCommand>(&s);
			}
		}
	}
}
