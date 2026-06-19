//! WebSocket message types for real-time communication.
//!
//! These types are shared between the WASM client and native server.
//! All messages use `#[serde(tag = "type", content = "payload")]` for
//! a `{"type": "...", "payload": {...}}` wire format.

use serde::{Deserialize, Serialize};

use crate::apps::deployments::server_fn::PreviewSummary;

/// Server-to-client WebSocket message.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", content = "payload")]
pub enum WsMessage {
	/// Deployment status update.
	DeploymentStatus(DeploymentStatusPayload),
	/// System-wide notification.
	SystemNotification(SystemNotificationPayload),
	/// Acknowledgement for a log stream subscription request.
	LogStreamAck(LogStreamAckPayload),
	/// Real-time build log event.
	BuildLog(BuildLogPayload),
	/// Application log entry.
	AppLog(AppLogPayload),
	/// Cluster agent health update.
	ClusterHealth(ClusterHealthPayload),
	/// Preview environment status update for one parent Project.
	PreviewStatusUpdate(ProjectPreviewUpdatePayload),
}

/// Build log event payload (streamed from gRPC BuildService).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BuildLogPayload {
	pub build_id: String,
	pub event_type: String,
	pub message: String,
	pub timestamp: String,
}

/// Application log entry payload (streamed from gRPC LogService).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppLogPayload {
	pub source: String,
	pub level: String,
	pub message: String,
	pub timestamp: String,
	pub metadata: Option<serde_json::Value>,
}

/// Cluster health update payload (from AgentRegistry).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClusterHealthPayload {
	pub cluster_name: String,
	pub agent_id: String,
	pub healthy: bool,
	pub cpu_usage_percent: f64,
	pub memory_usage_percent: f64,
	pub pod_count: u32,
	pub timestamp: String,
}

/// Preview status update payload.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProjectPreviewUpdatePayload {
	pub project_name: String,
	pub previews: Vec<PreviewSummary>,
	pub timestamp: String,
}

/// Deployment status update payload.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeploymentStatusPayload {
	pub deployment_id: String,
	pub name: String,
	pub namespace: String,
	pub status: DeploymentState,
	pub ready_replicas: u32,
	pub desired_replicas: u32,
	pub message: Option<String>,
	/// ISO 8601 timestamp string (not `chrono::DateTime` for WASM compat).
	pub timestamp: String,
}

/// Deployment lifecycle state.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum DeploymentState {
	Deploying,
	Running,
	Degraded,
	Failed,
	Stopped,
}

/// System notification payload.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SystemNotificationPayload {
	pub id: String,
	pub level: NotificationLevel,
	pub title: String,
	pub message: String,
	/// ISO 8601 timestamp string.
	pub timestamp: String,
}

/// Notification severity level.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum NotificationLevel {
	Info,
	Warning,
	Critical,
}

/// Acknowledgement payload for log stream subscription requests.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LogStreamAckPayload {
	pub acknowledged: bool,
	pub message: String,
}

/// Client-to-server WebSocket message.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", content = "payload")]
pub enum WsClientMessage {
	/// Subscribe to deployment status updates.
	Subscribe { deployment_ids: Vec<String> },
	/// Unsubscribe from deployment status updates.
	Unsubscribe { deployment_ids: Vec<String> },
	/// Subscribe to build log events.
	SubscribeBuildLogs { build_id: String },
	/// Subscribe to application log stream for an authorized deployment.
	SubscribeAppLogs { deployment_id: String },
	/// Unsubscribe from all log streams.
	UnsubscribeLogs,
	/// Subscribe to preview status updates for parent Projects.
	SubscribePreviews { project_names: Vec<String> },
	/// Unsubscribe from preview status updates for parent Projects.
	UnsubscribePreviews { project_names: Vec<String> },
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_ws_message_deployment_status_serializes_with_tagged_format() {
		// Arrange
		let msg = WsMessage::DeploymentStatus(DeploymentStatusPayload {
			deployment_id: "dep-1".to_string(),
			name: "my-app".to_string(),
			namespace: "default".to_string(),
			status: DeploymentState::Running,
			ready_replicas: 3,
			desired_replicas: 3,
			message: None,
			timestamp: "2026-03-22T00:00:00Z".to_string(),
		});

		// Act
		let json = serde_json::to_string(&msg).unwrap();
		let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(parsed["type"], "DeploymentStatus");
		assert_eq!(parsed["payload"]["deployment_id"], "dep-1");
		assert_eq!(parsed["payload"]["status"], "Running");
	}

	#[rstest]
	fn test_ws_message_system_notification_roundtrip() {
		// Arrange
		let msg = WsMessage::SystemNotification(SystemNotificationPayload {
			id: "notif-1".to_string(),
			level: NotificationLevel::Warning,
			title: "High CPU".to_string(),
			message: "Cluster cpu usage above 90%".to_string(),
			timestamp: "2026-03-22T00:00:00Z".to_string(),
		});

		// Act
		let json = serde_json::to_string(&msg).unwrap();
		let roundtrip: WsMessage = serde_json::from_str(&json).unwrap();

		// Assert
		match roundtrip {
			WsMessage::SystemNotification(p) => {
				assert_eq!(p.level, NotificationLevel::Warning);
				assert_eq!(p.title, "High CPU");
			}
			_ => panic!("expected SystemNotification"),
		}
	}

	#[rstest]
	fn test_ws_client_message_subscribe_roundtrip() {
		// Arrange
		let msg = WsClientMessage::Subscribe {
			deployment_ids: vec!["dep-1".to_string(), "dep-2".to_string()],
		};

		// Act
		let json = serde_json::to_string(&msg).unwrap();
		let roundtrip: WsClientMessage = serde_json::from_str(&json).unwrap();

		// Assert
		match roundtrip {
			WsClientMessage::Subscribe { deployment_ids } => {
				assert_eq!(deployment_ids, vec!["dep-1", "dep-2"]);
			}
			_ => panic!("expected Subscribe"),
		}
	}

	#[rstest]
	fn test_ws_client_message_subscribe_app_logs_roundtrip() {
		// Arrange
		let msg = WsClientMessage::SubscribeAppLogs {
			deployment_id: "42".to_string(),
		};

		// Act
		let json = serde_json::to_string(&msg).unwrap();
		let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
		let roundtrip: WsClientMessage = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(parsed["type"], "SubscribeAppLogs");
		assert_eq!(parsed["payload"]["deployment_id"], "42");
		match roundtrip {
			WsClientMessage::SubscribeAppLogs { deployment_id } => {
				assert_eq!(deployment_id, "42");
			}
			_ => panic!("expected SubscribeAppLogs"),
		}
	}

	#[rstest]
	fn test_ws_message_preview_status_update_roundtrip() {
		// Arrange
		let msg = WsMessage::PreviewStatusUpdate(ProjectPreviewUpdatePayload {
			project_name: "api".to_string(),
			previews: vec![PreviewSummary {
				name: "api-pr-42".to_string(),
				pr_number: "42".to_string(),
				url: Some("https://api-pr-42.preview.example.com".to_string()),
				phase: Some("running".to_string()),
				ready_replicas: Some(1),
				last_activity: Some("2026-06-19T00:00:00Z".to_string()),
			}],
			timestamp: "2026-06-19T00:00:00Z".to_string(),
		});

		// Act
		let json = serde_json::to_string(&msg).unwrap();
		let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
		let roundtrip: WsMessage = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(parsed["type"], "PreviewStatusUpdate");
		assert_eq!(parsed["payload"]["project_name"], "api");
		match roundtrip {
			WsMessage::PreviewStatusUpdate(payload) => {
				assert_eq!(payload.project_name, "api");
				assert_eq!(payload.previews[0].name, "api-pr-42");
			}
			_ => panic!("expected PreviewStatusUpdate"),
		}
	}

	#[rstest]
	fn test_ws_client_message_subscribe_previews_roundtrip() {
		// Arrange
		let msg = WsClientMessage::SubscribePreviews {
			project_names: vec!["api".to_string(), "web".to_string()],
		};

		// Act
		let json = serde_json::to_string(&msg).unwrap();
		let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
		let roundtrip: WsClientMessage = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(parsed["type"], "SubscribePreviews");
		assert_eq!(parsed["payload"]["project_names"][0], "api");
		match roundtrip {
			WsClientMessage::SubscribePreviews { project_names } => {
				assert_eq!(project_names, vec!["api", "web"]);
			}
			_ => panic!("expected SubscribePreviews"),
		}
	}

	#[rstest]
	fn test_invalid_json_returns_error() {
		// Arrange
		let bad_json = r#"{"type": "Unknown", "payload": {}}"#;

		// Act
		let result = serde_json::from_str::<WsMessage>(bad_json);

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	#[case(DeploymentState::Deploying)]
	#[case(DeploymentState::Running)]
	#[case(DeploymentState::Degraded)]
	#[case(DeploymentState::Failed)]
	#[case(DeploymentState::Stopped)]
	fn test_deployment_state_all_variants_roundtrip(#[case] state: DeploymentState) {
		// Act
		let json = serde_json::to_string(&state).unwrap();
		let roundtrip: DeploymentState = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(roundtrip, state);
	}

	#[rstest]
	#[case(NotificationLevel::Info)]
	#[case(NotificationLevel::Warning)]
	#[case(NotificationLevel::Critical)]
	fn test_notification_level_all_variants_roundtrip(#[case] level: NotificationLevel) {
		// Act
		let json = serde_json::to_string(&level).unwrap();
		let roundtrip: NotificationLevel = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(roundtrip, level);
	}

	#[rstest]
	#[case(None)]
	#[case(Some("".to_string()))]
	fn test_deployment_status_payload_optional_message(#[case] message: Option<String>) {
		// Arrange
		let payload = DeploymentStatusPayload {
			deployment_id: "dep-1".to_string(),
			name: "my-app".to_string(),
			namespace: "default".to_string(),
			status: DeploymentState::Running,
			ready_replicas: 1,
			desired_replicas: 1,
			message: message.clone(),
			timestamp: "2026-03-22T00:00:00Z".to_string(),
		};

		// Act
		let json = serde_json::to_string(&payload).unwrap();
		let roundtrip: DeploymentStatusPayload = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(roundtrip.message, message);
	}

	// `proptest` is only wired into native dev-deps because `dashboard`
	// also runs `wasm-bindgen-test` browser tests (#574); pulling
	// `proptest` into the WASM dev-dep set would bloat the wasm test
	// build. The fuzz coverage is target-agnostic — running it on the
	// native target is sufficient. Refs #574.
	#[cfg(not(target_arch = "wasm32"))]
	mod property_tests {
		use super::super::*;
		use proptest::prelude::*;

		proptest! {
			#[test]
			fn test_ws_client_message_fuzz_no_panic(s in "\\PC{0,500}") {
				// Any string should never panic, only return Ok or Err
				let _ = serde_json::from_str::<WsMessage>(&s);
				let _ = serde_json::from_str::<WsClientMessage>(&s);
			}
		}
	}
}
