//! gRPC Build Service server and client implementations.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use prost_types::Timestamp;
use tokio::sync::mpsc;
use tokio_stream::{Stream, StreamExt, wrappers::ReceiverStream};
use tonic::{Request, Response, Status};

use reinhardt_cloud_core::traits::BuildService;
use reinhardt_cloud_proto::build as pb;
use reinhardt_cloud_proto::common::StatusResponse;
use reinhardt_cloud_types::build::{
	self as domain, BuildEvent, BuildPhase, BuildRequest, EnvVar,
};

// --- Proto <-> Domain conversions ---

fn timestamp_from_chrono(dt: chrono::DateTime<chrono::Utc>) -> Option<Timestamp> {
	Some(Timestamp {
		seconds: dt.timestamp(),
		nanos: dt.timestamp_subsec_nanos() as i32,
	})
}

fn build_phase_to_proto(phase: &BuildPhase) -> i32 {
	match phase {
		BuildPhase::Queued => pb::BuildPhase::Queued as i32,
		BuildPhase::Pulling => pb::BuildPhase::Pulling as i32,
		BuildPhase::Building => pb::BuildPhase::Building as i32,
		BuildPhase::Pushing => pb::BuildPhase::Pushing as i32,
		BuildPhase::Finalizing => pb::BuildPhase::Finalizing as i32,
	}
}

fn domain_event_to_proto(event: &BuildEvent) -> pb::BuildEvent {
	let event_oneof = match event {
		BuildEvent::Log { message, timestamp } => Some(pb::build_event::Event::Log(pb::BuildLog {
			message: message.clone(),
			timestamp: timestamp_from_chrono(*timestamp),
		})),
		BuildEvent::PhaseChange { phase, timestamp } => {
			Some(pb::build_event::Event::PhaseChange(pb::BuildPhaseChange {
				phase: build_phase_to_proto(phase),
				timestamp: timestamp_from_chrono(*timestamp),
			}))
		}
		BuildEvent::ArtifactReady {
			artifact_url,
			digest,
			timestamp,
		} => Some(pb::build_event::Event::ArtifactReady(
			pb::BuildArtifactReady {
				artifact_url: artifact_url.clone(),
				digest: digest.clone(),
				timestamp: timestamp_from_chrono(*timestamp),
			},
		)),
		BuildEvent::Error { message, timestamp } => {
			Some(pb::build_event::Event::Error(pb::BuildError {
				message: message.clone(),
				timestamp: timestamp_from_chrono(*timestamp),
			}))
		}
		BuildEvent::Complete { success, timestamp } => {
			Some(pb::build_event::Event::Complete(pb::BuildComplete {
				success: *success,
				timestamp: timestamp_from_chrono(*timestamp),
			}))
		}
	};
	pb::BuildEvent { event: event_oneof }
}

fn proto_request_to_domain(req: &pb::StartBuildRequest) -> BuildRequest {
	BuildRequest {
		app_name: req.app_name.clone(),
		image: req.image.clone(),
		env_vars: req
			.env_vars
			.iter()
			.map(|e| EnvVar {
				key: e.key.clone(),
				value: e.value.clone(),
			})
			.collect(),
		dockerfile: req.dockerfile.clone(),
		context_path: req.context_path.clone(),
	}
}

// --- gRPC Server ---

/// gRPC server implementation wrapping a `BuildService` trait object.
pub struct BuildServiceGrpc {
	service: Arc<dyn BuildService>,
}

impl BuildServiceGrpc {
	pub fn new(service: Arc<dyn BuildService>) -> Self {
		Self { service }
	}
}

#[tonic::async_trait]
impl pb::build_service_server::BuildService for BuildServiceGrpc {
	type StartBuildStream =
		Pin<Box<dyn Stream<Item = Result<pb::BuildEvent, Status>> + Send + 'static>>;
	type StreamBuildLogsStream =
		Pin<Box<dyn Stream<Item = Result<pb::BuildLog, Status>> + Send + 'static>>;

	async fn start_build(
		&self,
		request: Request<pb::StartBuildRequest>,
	) -> Result<Response<Self::StartBuildStream>, Status> {
		let domain_req = proto_request_to_domain(request.get_ref());

		let stream = self
			.service
			.start_build(domain_req)
			.await
			.map_err(|e| Status::internal(e.to_string()))?;

		let mapped = stream.map(|result| match result {
			Ok(event) => Ok(domain_event_to_proto(&event)),
			Err(e) => Err(Status::internal(e.to_string())),
		});

		Ok(Response::new(Box::pin(mapped)))
	}

	async fn cancel_build(
		&self,
		request: Request<pb::CancelBuildRequest>,
	) -> Result<Response<StatusResponse>, Status> {
		let build_id = request
			.get_ref()
			.build_id
			.parse()
			.map_err(|e| Status::invalid_argument(format!("Invalid build_id: {e}")))?;

		self.service
			.cancel_build(build_id)
			.await
			.map_err(|e| Status::internal(e.to_string()))?;

		Ok(Response::new(StatusResponse {
			success: true,
			message: "Build cancelled".to_string(),
		}))
	}

	async fn get_build_status(
		&self,
		request: Request<pb::GetBuildStatusRequest>,
	) -> Result<Response<pb::BuildStatusResponse>, Status> {
		let build_id = request
			.get_ref()
			.build_id
			.parse()
			.map_err(|e| Status::invalid_argument(format!("Invalid build_id: {e}")))?;

		let status = self
			.service
			.get_build_status(build_id)
			.await
			.map_err(|e| Status::not_found(e.to_string()))?;

		Ok(Response::new(pb::BuildStatusResponse {
			build_id: status.build_id.to_string(),
			app_name: status.app_name,
			phase: build_phase_to_proto(&status.phase),
			completed: status.completed,
			success: status.success,
			started_at: timestamp_from_chrono(status.started_at),
			completed_at: status.completed_at.and_then(timestamp_from_chrono),
		}))
	}

	async fn stream_build_logs(
		&self,
		_request: Request<pb::StreamBuildLogsRequest>,
	) -> Result<Response<Self::StreamBuildLogsStream>, Status> {
		// Placeholder — full implementation requires build log persistence
		let (_, rx) = mpsc::channel::<Result<pb::BuildLog, Status>>(1);
		Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
	}
}

// --- gRPC Client ---

/// gRPC client implementing the `BuildService` trait via remote calls.
pub struct GrpcBuildClient {
	client: pb::build_service_client::BuildServiceClient<tonic::transport::Channel>,
}

impl GrpcBuildClient {
	/// Connect to a remote Build Service.
	pub async fn connect(endpoint: String) -> Result<Self, tonic::transport::Error> {
		let client = pb::build_service_client::BuildServiceClient::connect(endpoint).await?;
		Ok(Self { client })
	}
}

#[async_trait]
impl BuildService for GrpcBuildClient {
	async fn start_build(
		&self,
		request: BuildRequest,
	) -> Result<
		Pin<Box<dyn Stream<Item = Result<domain::BuildEvent, reinhardt_cloud_core::ApiError>> + Send>>,
		reinhardt_cloud_core::ApiError,
	> {
		let proto_req = pb::StartBuildRequest {
			app_name: request.app_name,
			image: request.image,
			env_vars: request
				.env_vars
				.into_iter()
				.map(|e| pb::EnvVar {
					key: e.key,
					value: e.value,
				})
				.collect(),
			dockerfile: request.dockerfile,
			context_path: request.context_path,
		};

		let response = self
			.client
			.clone()
			.start_build(proto_req)
			.await
			.map_err(|e| reinhardt_cloud_core::ApiError::Internal(e.to_string()))?;

		let stream = response.into_inner().filter_map(|result| match result {
			Ok(event) => proto_event_to_domain(&event).map(Ok),
			Err(e) => Some(Err(reinhardt_cloud_core::ApiError::Internal(e.to_string()))),
		});

		Ok(Box::pin(stream))
	}

	async fn cancel_build(&self, build_id: uuid::Uuid) -> Result<(), reinhardt_cloud_core::ApiError> {
		self.client
			.clone()
			.cancel_build(pb::CancelBuildRequest {
				build_id: build_id.to_string(),
			})
			.await
			.map_err(|e| reinhardt_cloud_core::ApiError::Internal(e.to_string()))?;
		Ok(())
	}

	async fn get_build_status(
		&self,
		build_id: uuid::Uuid,
	) -> Result<domain::BuildStatus, reinhardt_cloud_core::ApiError> {
		let resp = self
			.client
			.clone()
			.get_build_status(pb::GetBuildStatusRequest {
				build_id: build_id.to_string(),
			})
			.await
			.map_err(|e| reinhardt_cloud_core::ApiError::Internal(e.to_string()))?
			.into_inner();

		Ok(domain::BuildStatus {
			build_id: resp
				.build_id
				.parse()
				.map_err(|e| reinhardt_cloud_core::ApiError::Internal(format!("Invalid UUID: {e}")))?,
			app_name: resp.app_name,
			phase: proto_phase_to_domain(resp.phase),
			completed: resp.completed,
			success: resp.success,
			started_at: proto_timestamp_to_chrono(resp.started_at),
			completed_at: resp.completed_at.map(|t| proto_timestamp_to_chrono(Some(t))),
		})
	}
}

fn proto_phase_to_domain(phase: i32) -> BuildPhase {
	match pb::BuildPhase::try_from(phase) {
		Ok(pb::BuildPhase::Queued) => BuildPhase::Queued,
		Ok(pb::BuildPhase::Pulling) => BuildPhase::Pulling,
		Ok(pb::BuildPhase::Building) => BuildPhase::Building,
		Ok(pb::BuildPhase::Pushing) => BuildPhase::Pushing,
		Ok(pb::BuildPhase::Finalizing) => BuildPhase::Finalizing,
		_ => BuildPhase::Queued,
	}
}

fn proto_timestamp_to_chrono(ts: Option<Timestamp>) -> chrono::DateTime<chrono::Utc> {
	ts.and_then(|t| {
		chrono::DateTime::from_timestamp(t.seconds, t.nanos.try_into().unwrap_or(0))
	})
	.unwrap_or_else(chrono::Utc::now)
}

fn proto_event_to_domain(event: &pb::BuildEvent) -> Option<BuildEvent> {
	match &event.event {
		Some(pb::build_event::Event::Log(log)) => Some(BuildEvent::Log {
			message: log.message.clone(),
			timestamp: proto_timestamp_to_chrono(log.timestamp.clone()),
		}),
		Some(pb::build_event::Event::PhaseChange(pc)) => Some(BuildEvent::PhaseChange {
			phase: proto_phase_to_domain(pc.phase),
			timestamp: proto_timestamp_to_chrono(pc.timestamp.clone()),
		}),
		Some(pb::build_event::Event::ArtifactReady(ar)) => Some(BuildEvent::ArtifactReady {
			artifact_url: ar.artifact_url.clone(),
			digest: ar.digest.clone(),
			timestamp: proto_timestamp_to_chrono(ar.timestamp.clone()),
		}),
		Some(pb::build_event::Event::Error(e)) => Some(BuildEvent::Error {
			message: e.message.clone(),
			timestamp: proto_timestamp_to_chrono(e.timestamp.clone()),
		}),
		Some(pb::build_event::Event::Complete(c)) => Some(BuildEvent::Complete {
			success: c.success,
			timestamp: proto_timestamp_to_chrono(c.timestamp.clone()),
		}),
		None => None,
	}
}
