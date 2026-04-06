//! gRPC Cluster Agent Service server and client implementations.

use std::pin::Pin;
use std::sync::Arc;

use prost_types::Timestamp;
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status, Streaming};

use reinhardt_cloud_core::traits::ClusterAgentService;
use reinhardt_cloud_proto::cluster_agent as pb;
use reinhardt_cloud_proto::common::StatusResponse;
use reinhardt_cloud_types::agent::{AgentCommand, AgentEvent, AgentHealth};

// --- Conversions ---

fn timestamp_from_chrono(dt: chrono::DateTime<chrono::Utc>) -> Option<Timestamp> {
	Some(Timestamp {
		seconds: dt.timestamp(),
		nanos: dt.timestamp_subsec_nanos() as i32,
	})
}

fn proto_timestamp_to_chrono(ts: Option<Timestamp>) -> chrono::DateTime<chrono::Utc> {
	ts.and_then(|t| chrono::DateTime::from_timestamp(t.seconds, t.nanos.try_into().unwrap_or(0)))
		.unwrap_or_else(chrono::Utc::now)
}

fn domain_command_to_proto(cmd: &AgentCommand) -> pb::AgentCommand {
	let command = match cmd {
		AgentCommand::Deploy {
			app_name,
			image,
			replicas,
		} => Some(pb::agent_command::Command::Deploy(pb::DeployCommand {
			app_name: app_name.clone(),
			image: image.clone(),
			replicas: *replicas,
		})),
		AgentCommand::Rollback { app_name, revision } => {
			Some(pb::agent_command::Command::Rollback(pb::RollbackCommand {
				app_name: app_name.clone(),
				revision: *revision,
			}))
		}
		AgentCommand::Scale { app_name, replicas } => {
			Some(pb::agent_command::Command::Scale(pb::ScaleCommand {
				app_name: app_name.clone(),
				replicas: *replicas,
			}))
		}
		AgentCommand::Restart { app_name } => {
			Some(pb::agent_command::Command::Restart(pb::RestartCommand {
				app_name: app_name.clone(),
			}))
		}
	};
	pb::AgentCommand { command }
}

fn proto_event_to_domain(event: &pb::AgentEvent) -> Option<AgentEvent> {
	match &event.event {
		Some(pb::agent_event::Event::Connected(c)) => Some(AgentEvent::Connected {
			agent_id: c.agent_id.parse().ok()?,
			cluster_name: c.cluster_name.clone(),
			timestamp: proto_timestamp_to_chrono(c.timestamp.clone()),
		}),
		Some(pb::agent_event::Event::DeployStatus(d)) => Some(AgentEvent::DeployStatus {
			app_name: d.app_name.clone(),
			success: d.success,
			message: d.message.clone(),
			timestamp: proto_timestamp_to_chrono(d.timestamp.clone()),
		}),
		Some(pb::agent_event::Event::Heartbeat(h)) => Some(AgentEvent::Heartbeat {
			agent_id: h.agent_id.parse().ok()?,
			timestamp: proto_timestamp_to_chrono(h.timestamp.clone()),
		}),
		Some(pb::agent_event::Event::Error(e)) => Some(AgentEvent::Error {
			message: e.message.clone(),
			timestamp: proto_timestamp_to_chrono(e.timestamp.clone()),
		}),
		None => None,
	}
}

// --- gRPC Server ---

/// gRPC server implementation wrapping a `ClusterAgentService` trait object.
pub struct AgentServiceGrpc {
	service: Arc<dyn ClusterAgentService>,
}

impl AgentServiceGrpc {
	pub fn new(service: Arc<dyn ClusterAgentService>) -> Self {
		Self { service }
	}
}

#[tonic::async_trait]
impl pb::agent_service_server::AgentService for AgentServiceGrpc {
	type AgentStreamStream =
		Pin<Box<dyn Stream<Item = Result<pb::AgentCommand, Status>> + Send + 'static>>;

	async fn agent_stream(
		&self,
		request: Request<Streaming<pb::AgentEvent>>,
	) -> Result<Response<Self::AgentStreamStream>, Status> {
		let incoming = request.into_inner();

		// Convert proto stream to domain stream
		let domain_stream = incoming.filter_map(|result| match result {
			Ok(event) => proto_event_to_domain(&event).map(Ok),
			Err(e) => Some(Err(reinhardt_cloud_core::ApiError::Internal(e.to_string()))),
		});

		let command_stream = self
			.service
			.agent_stream(Box::pin(domain_stream))
			.await
			.map_err(|e| Status::internal(e.to_string()))?;

		let mapped = command_stream.map(|result| match result {
			Ok(cmd) => Ok(domain_command_to_proto(&cmd)),
			Err(e) => Err(Status::internal(e.to_string())),
		});

		Ok(Response::new(Box::pin(mapped)))
	}

	async fn report_health(
		&self,
		request: Request<pb::AgentHealthReport>,
	) -> Result<Response<StatusResponse>, Status> {
		let report = request.into_inner();
		let agent_id = report
			.agent_id
			.parse()
			.map_err(|e| Status::invalid_argument(format!("Invalid agent_id: {e}")))?;

		let health = AgentHealth {
			agent_id,
			healthy: report.healthy,
			cpu_usage_percent: report.cpu_usage_percent,
			memory_usage_percent: report.memory_usage_percent,
			pod_count: report.pod_count,
			reported_at: proto_timestamp_to_chrono(report.reported_at),
		};

		self.service
			.report_health(health)
			.await
			.map_err(|e| Status::internal(e.to_string()))?;

		Ok(Response::new(StatusResponse {
			success: true,
			message: "Health reported".to_string(),
		}))
	}

	async fn report_deploy_status(
		&self,
		_request: Request<pb::AgentDeployStatus>,
	) -> Result<Response<StatusResponse>, Status> {
		Ok(Response::new(StatusResponse {
			success: true,
			message: "Deploy status received".to_string(),
		}))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::{TimeZone, Utc};
	use rstest::rstest;
	use uuid::Uuid;

	/// Helper: create a fixed timestamp for deterministic testing.
	fn fixed_timestamp() -> chrono::DateTime<chrono::Utc> {
		Utc.with_ymd_and_hms(2025, 6, 15, 12, 30, 45).unwrap()
	}

	/// Helper: create a prost Timestamp from a chrono DateTime.
	fn to_proto_ts(dt: chrono::DateTime<chrono::Utc>) -> Timestamp {
		Timestamp {
			seconds: dt.timestamp(),
			nanos: dt.timestamp_subsec_nanos() as i32,
		}
	}

	// --- domain_command_to_proto: all 4 command variants (field-by-field) ---

	#[rstest]
	fn test_domain_command_to_proto_deploy() {
		// Arrange
		let cmd = AgentCommand::Deploy {
			app_name: "web-app".to_string(),
			image: "registry.io/web:v3".to_string(),
			replicas: 5,
		};

		// Act
		let proto = domain_command_to_proto(&cmd);

		// Assert
		let deploy = match proto.command {
			Some(pb::agent_command::Command::Deploy(d)) => d,
			other => panic!("Expected Deploy variant, got {other:?}"),
		};
		assert_eq!(deploy.app_name, "web-app");
		assert_eq!(deploy.image, "registry.io/web:v3");
		assert_eq!(deploy.replicas, 5);
	}

	#[rstest]
	fn test_domain_command_to_proto_rollback() {
		// Arrange
		let cmd = AgentCommand::Rollback {
			app_name: "api-svc".to_string(),
			revision: 42,
		};

		// Act
		let proto = domain_command_to_proto(&cmd);

		// Assert
		let rollback = match proto.command {
			Some(pb::agent_command::Command::Rollback(r)) => r,
			other => panic!("Expected Rollback variant, got {other:?}"),
		};
		assert_eq!(rollback.app_name, "api-svc");
		assert_eq!(rollback.revision, 42);
	}

	#[rstest]
	fn test_domain_command_to_proto_scale() {
		// Arrange
		let cmd = AgentCommand::Scale {
			app_name: "worker".to_string(),
			replicas: 10,
		};

		// Act
		let proto = domain_command_to_proto(&cmd);

		// Assert
		let scale = match proto.command {
			Some(pb::agent_command::Command::Scale(s)) => s,
			other => panic!("Expected Scale variant, got {other:?}"),
		};
		assert_eq!(scale.app_name, "worker");
		assert_eq!(scale.replicas, 10);
	}

	#[rstest]
	fn test_domain_command_to_proto_restart() {
		// Arrange
		let cmd = AgentCommand::Restart {
			app_name: "cache".to_string(),
		};

		// Act
		let proto = domain_command_to_proto(&cmd);

		// Assert
		let restart = match proto.command {
			Some(pb::agent_command::Command::Restart(r)) => r,
			other => panic!("Expected Restart variant, got {other:?}"),
		};
		assert_eq!(restart.app_name, "cache");
	}

	// --- proto_event_to_domain: all 4 event variants (field-by-field) ---

	#[rstest]
	fn test_proto_event_to_domain_connected() {
		// Arrange
		let ts = fixed_timestamp();
		let agent_id = Uuid::new_v4();
		let proto = pb::AgentEvent {
			event: Some(pb::agent_event::Event::Connected(pb::AgentConnected {
				agent_id: agent_id.to_string(),
				cluster_name: "prod-cluster".to_string(),
				timestamp: Some(to_proto_ts(ts)),
			})),
		};

		// Act
		let domain = proto_event_to_domain(&proto).unwrap();

		// Assert
		match domain {
			AgentEvent::Connected {
				agent_id: aid,
				cluster_name,
				timestamp,
			} => {
				assert_eq!(aid, agent_id);
				assert_eq!(cluster_name, "prod-cluster");
				assert_eq!(timestamp, ts);
			}
			other => panic!("Expected Connected variant, got {other:?}"),
		}
	}

	#[rstest]
	fn test_proto_event_to_domain_deploy_status() {
		// Arrange
		let ts = fixed_timestamp();
		let proto = pb::AgentEvent {
			event: Some(pb::agent_event::Event::DeployStatus(
				pb::AgentDeployStatus {
					app_name: "api".to_string(),
					success: true,
					message: "all pods ready".to_string(),
					timestamp: Some(to_proto_ts(ts)),
				},
			)),
		};

		// Act
		let domain = proto_event_to_domain(&proto).unwrap();

		// Assert
		match domain {
			AgentEvent::DeployStatus {
				app_name,
				success,
				message,
				timestamp,
			} => {
				assert_eq!(app_name, "api");
				assert!(success);
				assert_eq!(message, "all pods ready");
				assert_eq!(timestamp, ts);
			}
			other => panic!("Expected DeployStatus variant, got {other:?}"),
		}
	}

	#[rstest]
	fn test_proto_event_to_domain_heartbeat() {
		// Arrange
		let ts = fixed_timestamp();
		let agent_id = Uuid::new_v4();
		let proto = pb::AgentEvent {
			event: Some(pb::agent_event::Event::Heartbeat(pb::AgentHeartbeat {
				agent_id: agent_id.to_string(),
				timestamp: Some(to_proto_ts(ts)),
			})),
		};

		// Act
		let domain = proto_event_to_domain(&proto).unwrap();

		// Assert
		match domain {
			AgentEvent::Heartbeat {
				agent_id: aid,
				timestamp,
			} => {
				assert_eq!(aid, agent_id);
				assert_eq!(timestamp, ts);
			}
			other => panic!("Expected Heartbeat variant, got {other:?}"),
		}
	}

	#[rstest]
	fn test_proto_event_to_domain_error() {
		// Arrange
		let ts = fixed_timestamp();
		let proto = pb::AgentEvent {
			event: Some(pb::agent_event::Event::Error(pb::AgentError {
				message: "node unreachable".to_string(),
				timestamp: Some(to_proto_ts(ts)),
			})),
		};

		// Act
		let domain = proto_event_to_domain(&proto).unwrap();

		// Assert
		match domain {
			AgentEvent::Error { message, timestamp } => {
				assert_eq!(message, "node unreachable");
				assert_eq!(timestamp, ts);
			}
			other => panic!("Expected Error variant, got {other:?}"),
		}
	}

	// --- proto_event_to_domain with None event ---

	#[rstest]
	fn test_proto_event_to_domain_none() {
		// Arrange
		let proto = pb::AgentEvent { event: None };

		// Act
		let result = proto_event_to_domain(&proto);

		// Assert
		assert!(result.is_none());
	}

	// --- Connected with invalid UUID -> None ---

	#[rstest]
	fn test_proto_event_connected_invalid_uuid() {
		// Arrange
		let proto = pb::AgentEvent {
			event: Some(pb::agent_event::Event::Connected(pb::AgentConnected {
				agent_id: "not-a-uuid".to_string(),
				cluster_name: "test".to_string(),
				timestamp: Some(to_proto_ts(fixed_timestamp())),
			})),
		};

		// Act
		let result = proto_event_to_domain(&proto);

		// Assert
		assert!(result.is_none());
	}

	// --- Heartbeat with invalid UUID -> None ---

	#[rstest]
	fn test_proto_event_heartbeat_invalid_uuid() {
		// Arrange
		let proto = pb::AgentEvent {
			event: Some(pb::agent_event::Event::Heartbeat(pb::AgentHeartbeat {
				agent_id: "invalid".to_string(),
				timestamp: Some(to_proto_ts(fixed_timestamp())),
			})),
		};

		// Act
		let result = proto_event_to_domain(&proto);

		// Assert
		assert!(result.is_none());
	}

	// --- Deploy with zero replicas preserved ---

	#[rstest]
	fn test_domain_command_deploy_zero_replicas() {
		// Arrange
		let cmd = AgentCommand::Deploy {
			app_name: "app".to_string(),
			image: "img:latest".to_string(),
			replicas: 0,
		};

		// Act
		let proto = domain_command_to_proto(&cmd);

		// Assert
		let deploy = match proto.command {
			Some(pb::agent_command::Command::Deploy(d)) => d,
			other => panic!("Expected Deploy, got {other:?}"),
		};
		assert_eq!(deploy.replicas, 0);
	}

	// --- Deploy with u32::MAX replicas preserved ---

	#[rstest]
	fn test_domain_command_deploy_max_replicas() {
		// Arrange
		let cmd = AgentCommand::Deploy {
			app_name: "app".to_string(),
			image: "img:latest".to_string(),
			replicas: u32::MAX,
		};

		// Act
		let proto = domain_command_to_proto(&cmd);

		// Assert
		let deploy = match proto.command {
			Some(pb::agent_command::Command::Deploy(d)) => d,
			other => panic!("Expected Deploy, got {other:?}"),
		};
		assert_eq!(deploy.replicas, u32::MAX);
	}

	// --- Empty app_name preserved ---

	#[rstest]
	fn test_domain_command_empty_app_name() {
		// Arrange
		let cmd = AgentCommand::Restart {
			app_name: "".to_string(),
		};

		// Act
		let proto = domain_command_to_proto(&cmd);

		// Assert
		let restart = match proto.command {
			Some(pb::agent_command::Command::Restart(r)) => r,
			other => panic!("Expected Restart, got {other:?}"),
		};
		assert_eq!(restart.app_name, "");
	}

	// --- Decision table: #[case] for each command variant -> correct oneof arm ---

	#[rstest]
	#[case(
		AgentCommand::Deploy { app_name: "a".into(), image: "i".into(), replicas: 1 },
		"Deploy"
	)]
	#[case(
		AgentCommand::Rollback { app_name: "a".into(), revision: 1 },
		"Rollback"
	)]
	#[case(
		AgentCommand::Scale { app_name: "a".into(), replicas: 1 },
		"Scale"
	)]
	#[case(
		AgentCommand::Restart { app_name: "a".into() },
		"Restart"
	)]
	fn test_command_variant_maps_to_correct_oneof(
		#[case] cmd: AgentCommand,
		#[case] expected_arm: &str,
	) {
		// Arrange — provided by #[case]

		// Act
		let proto = domain_command_to_proto(&cmd);

		// Assert
		let arm_name = match &proto.command {
			Some(pb::agent_command::Command::Deploy(_)) => "Deploy",
			Some(pb::agent_command::Command::Rollback(_)) => "Rollback",
			Some(pb::agent_command::Command::Scale(_)) => "Scale",
			Some(pb::agent_command::Command::Restart(_)) => "Restart",
			None => "None",
		};
		assert_eq!(arm_name, expected_arm);
	}
}
