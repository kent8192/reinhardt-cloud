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
	ts.and_then(|t| {
		chrono::DateTime::from_timestamp(t.seconds, t.nanos.try_into().unwrap_or(0))
	})
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
		AgentCommand::Scale {
			app_name,
			replicas,
		} => Some(pb::agent_command::Command::Scale(pb::ScaleCommand {
			app_name: app_name.clone(),
			replicas: *replicas,
		})),
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
