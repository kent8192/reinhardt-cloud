//! gRPC Cluster Agent Service server and client implementations.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use prost_types::Timestamp;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};

use reinhardt_cloud_core::ApiError;
use reinhardt_cloud_core::traits::ClusterAgentService;
use reinhardt_cloud_proto::cluster_agent as pb;
use reinhardt_cloud_proto::common::StatusResponse;
use reinhardt_cloud_types::agent::{
	AgentCommand, AgentEvent, AgentHealth, AgentInfo, DeployStatusReport,
};
use uuid::Uuid;

use crate::agent_claims::AgentClaims;
use crate::registry::AgentRegistry;

// --- Conversions ---

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
			timestamp: proto_timestamp_to_chrono(c.timestamp),
		}),
		Some(pb::agent_event::Event::DeployStatus(d)) => Some(AgentEvent::DeployStatus {
			app_name: d.app_name.clone(),
			success: d.success,
			message: d.message.clone(),
			timestamp: proto_timestamp_to_chrono(d.timestamp),
		}),
		Some(pb::agent_event::Event::Heartbeat(h)) => Some(AgentEvent::Heartbeat {
			agent_id: h.agent_id.parse().ok()?,
			timestamp: proto_timestamp_to_chrono(h.timestamp),
		}),
		Some(pb::agent_event::Event::Error(e)) => Some(AgentEvent::Error {
			message: e.message.clone(),
			timestamp: proto_timestamp_to_chrono(e.timestamp),
		}),
		Some(pb::agent_event::Event::CommandStatus(s)) => Some(AgentEvent::CommandStatus {
			app_name: s.app_name.clone(),
			command_type: s.command_type.clone(),
			success: s.success,
			message: s.message.clone(),
			timestamp: proto_timestamp_to_chrono(s.timestamp),
		}),
		None => None,
	}
}

// --- Error mapping ---

fn api_error_to_status(e: ApiError) -> Status {
	match e {
		ApiError::NotFound(msg) => Status::not_found(msg),
		ApiError::BadRequest(msg) => Status::invalid_argument(msg),
		ApiError::Unauthorized(msg) => Status::unauthenticated(msg),
		ApiError::Internal(msg) => Status::internal(msg),
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
		// Extract the authenticated cluster_id from AgentJwtInterceptor
		// before consuming the request body. The interceptor is wired in
		// `dashboard::config::grpc::start_grpc_server`; absent claims here
		// indicate the interceptor was not installed (test or misconfig)
		// and the implementation should fall back to its unauthenticated
		// path.
		let cluster_id = request
			.extensions()
			.get::<AgentClaims>()
			.and_then(|claims| Uuid::parse_str(&claims.cluster_id).ok());

		let incoming = request.into_inner();

		// Convert proto stream to domain stream
		let domain_stream = incoming.filter_map(|result| match result {
			Ok(event) => proto_event_to_domain(&event).map(Ok),
			Err(e) => Some(Err(reinhardt_cloud_core::ApiError::Internal(e.to_string()))),
		});

		let command_stream = self
			.service
			.agent_stream_authenticated(Box::pin(domain_stream), cluster_id)
			.await
			.map_err(api_error_to_status)?;

		let mapped = command_stream.map(|result| match result {
			Ok(cmd) => Ok(domain_command_to_proto(&cmd)),
			Err(e) => Err(api_error_to_status(e)),
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
			.map_err(api_error_to_status)?;

		Ok(Response::new(StatusResponse {
			success: true,
			message: "Health reported".to_string(),
		}))
	}

	async fn report_deploy_status(
		&self,
		request: Request<pb::AgentDeployStatus>,
	) -> Result<Response<StatusResponse>, Status> {
		let status = request.into_inner();
		let reported_at = proto_timestamp_to_chrono(status.timestamp);

		tracing::info!(
			app_name = %status.app_name,
			success = status.success,
			message = %status.message,
			reported_at = %reported_at,
			"Received deploy status report"
		);

		let report = DeployStatusReport {
			app_name: status.app_name,
			success: status.success,
			message: status.message,
			reported_at,
		};

		self.service
			.report_deploy_status(report)
			.await
			.map_err(api_error_to_status)?;

		Ok(Response::new(StatusResponse {
			success: true,
			message: "Deploy status received".to_string(),
		}))
	}
}

// --- Registry-backed ClusterAgentService ---

/// Real implementation of `ClusterAgentService` backed by `AgentRegistry`.
///
/// This replaces the previous `MockClusterAgentService` used by the
/// dashboard gRPC server. On `agent_stream`, each connected agent's
/// outbound command channel is registered with the agent registry so
/// subsequent REST/gRPC control-plane requests can route
/// Deploy/Rollback/Scale/Restart commands to the correct cluster via
/// `AgentRegistry::send_command_to_cluster`.
pub struct RegistryBackedAgentService {
	registry: Arc<AgentRegistry>,
}

impl RegistryBackedAgentService {
	/// Create a service backed by the given agent registry singleton.
	pub fn new(registry: Arc<AgentRegistry>) -> Self {
		Self { registry }
	}

	/// Expose the underlying registry for admin/inspection endpoints.
	pub fn registry(&self) -> Arc<AgentRegistry> {
		self.registry.clone()
	}
}

#[async_trait]
impl ClusterAgentService for RegistryBackedAgentService {
	async fn agent_stream(
		&self,
		agent_events: Pin<Box<dyn Stream<Item = Result<AgentEvent, ApiError>> + Send>>,
	) -> Result<Pin<Box<dyn Stream<Item = Result<AgentCommand, ApiError>> + Send>>, ApiError> {
		// Consume the first event from the agent to identify the cluster/agent
		// binding. Subsequent events (heartbeats, deploy status) are processed
		// in the background.
		let mut events = agent_events;

		let (out_tx, out_rx) = mpsc::channel::<Result<AgentCommand, ApiError>>(64);

		// The Connected event is expected first; without it we cannot
		// associate a command channel with an agent_id.
		let first = events.next().await;
		let Some(Ok(AgentEvent::Connected {
			agent_id,
			cluster_name,
			timestamp,
		})) = first
		else {
			// Peer disconnected before announcing itself, or sent a
			// non-Connected event as the first message.
			return Err(ApiError::BadRequest(
				"Agent must send Connected event first".to_string(),
			));
		};

		let info = AgentInfo {
			agent_id,
			cluster_name,
			node_name: String::new(),
			version: String::new(),
			last_seen: timestamp,
		};

		// `agent_stream_authenticated` (below) re-routes through this
		// method with the cluster binding already applied; the plain
		// `agent_stream` path is reached only when no JWT claims were
		// injected by the interceptor (tests, misconfig).
		let mut command_rx = self.registry.register(info);
		let registry = self.registry.clone();
		let agent_id_copy = agent_id;

		// Forward commands from the registry to the outbound stream.
		tokio::spawn(async move {
			while let Some(cmd) = command_rx.recv().await {
				if out_tx.send(Ok(cmd)).await.is_err() {
					break;
				}
			}
			registry.unregister(&agent_id_copy);
		});

		// Consume agent-side events (heartbeat, deploy status) asynchronously.
		let registry_events = self.registry.clone();
		let agent_id_events = agent_id;
		tokio::spawn(async move {
			while let Some(result) = events.next().await {
				match result {
					Ok(AgentEvent::Heartbeat { agent_id: id, .. }) => {
						registry_events.heartbeat(&id);
					}
					Ok(AgentEvent::Error { .. }) | Ok(AgentEvent::DeployStatus { .. }) => {
						// Logged by the gRPC layer; no extra registry state.
					}
					Ok(_) => {}
					Err(_) => break,
				}
			}
			registry_events.unregister(&agent_id_events);
		});

		Ok(Box::pin(ReceiverStream::new(out_rx)))
	}

	async fn agent_stream_authenticated(
		&self,
		agent_events: Pin<Box<dyn Stream<Item = Result<AgentEvent, ApiError>> + Send>>,
		cluster_id: Option<Uuid>,
	) -> Result<Pin<Box<dyn Stream<Item = Result<AgentCommand, ApiError>> + Send>>, ApiError> {
		// Without an authenticated cluster_id we cannot route
		// `send_command_to_cluster` to this agent — refuse the connection
		// rather than silently registering an unroutable agent.
		let Some(cluster_id) = cluster_id else {
			return Err(ApiError::Unauthorized(
				"Agent identity required: missing or invalid JWT claims".to_string(),
			));
		};

		let mut events = agent_events;
		let (out_tx, out_rx) = mpsc::channel::<Result<AgentCommand, ApiError>>(64);

		let first = events.next().await;
		let Some(Ok(AgentEvent::Connected {
			agent_id,
			cluster_name,
			timestamp,
		})) = first
		else {
			return Err(ApiError::BadRequest(
				"Agent must send Connected event first".to_string(),
			));
		};

		let info = AgentInfo {
			agent_id,
			cluster_name,
			node_name: String::new(),
			version: String::new(),
			last_seen: timestamp,
		};

		// Bind the agent to its authenticated cluster_id so
		// `AgentRegistry::send_command_to_cluster` reaches it.
		let mut command_rx = self.registry.register_with_cluster(info, cluster_id);
		let registry = self.registry.clone();
		let agent_id_copy = agent_id;

		tokio::spawn(async move {
			while let Some(cmd) = command_rx.recv().await {
				if out_tx.send(Ok(cmd)).await.is_err() {
					break;
				}
			}
			registry.unregister(&agent_id_copy);
		});

		let registry_events = self.registry.clone();
		let agent_id_events = agent_id;
		tokio::spawn(async move {
			while let Some(result) = events.next().await {
				match result {
					Ok(AgentEvent::Heartbeat { agent_id: id, .. }) => {
						registry_events.heartbeat(&id);
					}
					Ok(AgentEvent::Error { .. }) | Ok(AgentEvent::DeployStatus { .. }) => {
						// Logged by the gRPC layer; no extra registry state.
					}
					Ok(_) => {}
					Err(_) => break,
				}
			}
			registry_events.unregister(&agent_id_events);
		});

		Ok(Box::pin(ReceiverStream::new(out_rx)))
	}

	async fn report_health(&self, health: AgentHealth) -> Result<(), ApiError> {
		let agent_id = health.agent_id;
		self.registry.update_health(&agent_id, health);
		Ok(())
	}

	async fn get_agent_health(&self, agent_id: Uuid) -> Result<AgentHealth, ApiError> {
		self.registry
			.get_health(&agent_id)
			.ok_or_else(|| ApiError::NotFound(format!("No health report for agent {agent_id}")))
	}

	async fn report_deploy_status(&self, _report: DeployStatusReport) -> Result<(), ApiError> {
		// Deploy status events flow through the agent_stream event pump; the
		// explicit `report_deploy_status` RPC is kept as a compatibility path.
		Ok(())
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
		let agent_id = Uuid::now_v7();
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
		let agent_id = Uuid::now_v7();
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

	#[rstest]
	fn test_proto_event_to_domain_command_status() {
		// Arrange
		let ts = fixed_timestamp();
		let proto = pb::AgentEvent {
			event: Some(pb::agent_event::Event::CommandStatus(
				pb::AgentCommandStatus {
					app_name: "web".to_string(),
					command_type: "rollback".to_string(),
					success: true,
					message: "Rollback applied".to_string(),
					timestamp: Some(to_proto_ts(ts)),
				},
			)),
		};

		// Act
		let domain = proto_event_to_domain(&proto).unwrap();

		// Assert
		match domain {
			AgentEvent::CommandStatus {
				app_name,
				command_type,
				success,
				message,
				timestamp,
			} => {
				assert_eq!(app_name, "web");
				assert_eq!(command_type, "rollback");
				assert!(success);
				assert_eq!(message, "Rollback applied");
				assert_eq!(timestamp, ts);
			}
			other => panic!("Expected CommandStatus variant, got {other:?}"),
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

	// --- RegistryBackedAgentService tests ---

	#[rstest]
	#[tokio::test]
	async fn test_registry_backed_report_health_writes_to_registry() {
		// Arrange
		let registry = Arc::new(AgentRegistry::new());
		let service = RegistryBackedAgentService::new(registry.clone());
		let agent_id = Uuid::now_v7();
		let info = AgentInfo {
			agent_id,
			cluster_name: "c".to_string(),
			node_name: "n".to_string(),
			version: "0.1".to_string(),
			last_seen: Utc::now(),
		};
		let _rx = registry.register(info);

		let health = AgentHealth {
			agent_id,
			healthy: true,
			cpu_usage_percent: 50.0,
			memory_usage_percent: 60.0,
			pod_count: 3,
			reported_at: Utc::now(),
		};

		// Act
		service.report_health(health).await.unwrap();

		// Assert
		let fetched = service.get_agent_health(agent_id).await.unwrap();
		assert_eq!(fetched.cpu_usage_percent, 50.0);
		assert_eq!(fetched.pod_count, 3);
	}

	#[rstest]
	#[tokio::test]
	async fn test_registry_backed_get_health_unknown_agent_is_not_found() {
		// Arrange
		let registry = Arc::new(AgentRegistry::new());
		let service = RegistryBackedAgentService::new(registry);

		// Act
		let result = service.get_agent_health(Uuid::now_v7()).await;

		// Assert
		assert!(matches!(result, Err(ApiError::NotFound(_))));
	}

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
