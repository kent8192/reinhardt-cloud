//! gRPC Build Service server and client implementations.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use prost_types::Timestamp;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status};

use reinhardt_cloud_core::ApiError;
use reinhardt_cloud_core::traits::BuildService;
use reinhardt_cloud_proto::build as pb;
use reinhardt_cloud_proto::common::StatusResponse;
use reinhardt_cloud_types::build::{self as domain, BuildEvent, BuildPhase, BuildRequest, EnvVar};

// --- Error mapping ---

fn api_error_to_status(e: ApiError) -> Status {
	match e {
		ApiError::NotFound(msg) => Status::not_found(msg),
		ApiError::BadRequest(msg) => Status::invalid_argument(msg),
		ApiError::Unauthorized(msg) => Status::unauthenticated(msg),
		ApiError::Internal(msg) => Status::internal(msg),
	}
}

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
			.map_err(api_error_to_status)?;

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
			.map_err(api_error_to_status)?;

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
			.map_err(api_error_to_status)?;

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
		request: Request<pb::StreamBuildLogsRequest>,
	) -> Result<Response<Self::StreamBuildLogsStream>, Status> {
		let build_id: uuid::Uuid = request
			.get_ref()
			.build_id
			.parse()
			.map_err(|e| Status::invalid_argument(format!("Invalid build_id: {e}")))?;

		let status = self
			.service
			.get_build_status(build_id)
			.await
			.map_err(api_error_to_status)?;

		let now = chrono::Utc::now();
		let mut logs = vec![Ok(pb::BuildLog {
			message: format!(
				"Build {} for app '{}' is in phase {:?}",
				status.build_id, status.app_name, status.phase
			),
			timestamp: timestamp_from_chrono(now),
		})];

		if status.completed {
			let outcome = match status.success {
				Some(true) => "succeeded",
				Some(false) => "failed",
				None => "completed (unknown outcome)",
			};
			logs.push(Ok(pb::BuildLog {
				message: format!("Build {} {outcome}", status.build_id),
				timestamp: timestamp_from_chrono(now),
			}));
		}

		Ok(Response::new(Box::pin(tokio_stream::iter(logs))))
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
		Pin<
			Box<
				dyn Stream<Item = Result<domain::BuildEvent, reinhardt_cloud_core::ApiError>>
					+ Send,
			>,
		>,
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

	async fn cancel_build(
		&self,
		build_id: uuid::Uuid,
	) -> Result<(), reinhardt_cloud_core::ApiError> {
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
			build_id: resp.build_id.parse().map_err(|e| {
				reinhardt_cloud_core::ApiError::Internal(format!("Invalid UUID: {e}"))
			})?,
			app_name: resp.app_name,
			phase: proto_phase_to_domain(resp.phase),
			completed: resp.completed,
			success: resp.success,
			started_at: proto_timestamp_to_chrono(resp.started_at),
			completed_at: resp
				.completed_at
				.map(|t| proto_timestamp_to_chrono(Some(t))),
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
	ts.and_then(|t| chrono::DateTime::from_timestamp(t.seconds, t.nanos.try_into().unwrap_or(0)))
		.unwrap_or_else(chrono::Utc::now)
}

fn proto_event_to_domain(event: &pb::BuildEvent) -> Option<BuildEvent> {
	match &event.event {
		Some(pb::build_event::Event::Log(log)) => Some(BuildEvent::Log {
			message: log.message.clone(),
			timestamp: proto_timestamp_to_chrono(log.timestamp),
		}),
		Some(pb::build_event::Event::PhaseChange(pc)) => Some(BuildEvent::PhaseChange {
			phase: proto_phase_to_domain(pc.phase),
			timestamp: proto_timestamp_to_chrono(pc.timestamp),
		}),
		Some(pb::build_event::Event::ArtifactReady(ar)) => Some(BuildEvent::ArtifactReady {
			artifact_url: ar.artifact_url.clone(),
			digest: ar.digest.clone(),
			timestamp: proto_timestamp_to_chrono(ar.timestamp),
		}),
		Some(pb::build_event::Event::Error(e)) => Some(BuildEvent::Error {
			message: e.message.clone(),
			timestamp: proto_timestamp_to_chrono(e.timestamp),
		}),
		Some(pb::build_event::Event::Complete(c)) => Some(BuildEvent::Complete {
			success: c.success,
			timestamp: proto_timestamp_to_chrono(c.timestamp),
		}),
		None => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::{DateTime, TimeZone, Utc};
	use prost_types::Timestamp;
	use rstest::rstest;

	/// Helper: create a fixed timestamp for deterministic testing.
	fn fixed_timestamp() -> DateTime<Utc> {
		Utc.with_ymd_and_hms(2025, 6, 15, 12, 30, 45).unwrap()
	}

	/// Helper: create a prost Timestamp from a chrono DateTime.
	fn to_proto_ts(dt: DateTime<Utc>) -> Timestamp {
		Timestamp {
			seconds: dt.timestamp(),
			nanos: dt.timestamp_subsec_nanos() as i32,
		}
	}

	// --- build_phase_to_proto: all 5 variants ---

	#[rstest]
	#[case(BuildPhase::Queued, pb::BuildPhase::Queued as i32)]
	#[case(BuildPhase::Pulling, pb::BuildPhase::Pulling as i32)]
	#[case(BuildPhase::Building, pb::BuildPhase::Building as i32)]
	#[case(BuildPhase::Pushing, pb::BuildPhase::Pushing as i32)]
	#[case(BuildPhase::Finalizing, pb::BuildPhase::Finalizing as i32)]
	fn test_build_phase_to_proto_all_variants(#[case] domain: BuildPhase, #[case] expected: i32) {
		// Arrange — provided by #[case]

		// Act
		let proto_val = build_phase_to_proto(&domain);

		// Assert
		assert_eq!(proto_val, expected);
	}

	// --- proto_phase_to_domain: all 5 variants ---

	#[rstest]
	#[case(pb::BuildPhase::Queued as i32, BuildPhase::Queued)]
	#[case(pb::BuildPhase::Pulling as i32, BuildPhase::Pulling)]
	#[case(pb::BuildPhase::Building as i32, BuildPhase::Building)]
	#[case(pb::BuildPhase::Pushing as i32, BuildPhase::Pushing)]
	#[case(pb::BuildPhase::Finalizing as i32, BuildPhase::Finalizing)]
	fn test_proto_phase_to_domain_all_variants(
		#[case] proto_val: i32,
		#[case] expected: BuildPhase,
	) {
		// Arrange — provided by #[case]

		// Act
		let domain = proto_phase_to_domain(proto_val);

		// Assert
		assert_eq!(domain, expected);
	}

	// --- proto_phase_to_domain: boundary values ---

	#[rstest]
	#[case(-1, BuildPhase::Queued)]
	#[case(0, BuildPhase::Queued)] // UNSPECIFIED -> Queued fallback
	#[case(6, BuildPhase::Queued)] // out of range -> Queued fallback
	#[case(99, BuildPhase::Queued)]
	#[case(i32::MAX, BuildPhase::Queued)]
	#[case(i32::MIN, BuildPhase::Queued)]
	fn test_proto_phase_boundary_values(#[case] proto_val: i32, #[case] expected: BuildPhase) {
		// Arrange — provided by #[case]

		// Act
		let domain = proto_phase_to_domain(proto_val);

		// Assert
		assert_eq!(domain, expected);
	}

	// --- domain_event_to_proto: all 5 event variants (field-by-field) ---

	#[rstest]
	fn test_domain_event_to_proto_log() {
		// Arrange
		let ts = fixed_timestamp();
		let event = BuildEvent::Log {
			message: "Step 1/5: FROM rust:1.80".to_string(),
			timestamp: ts,
		};

		// Act
		let proto = domain_event_to_proto(&event);

		// Assert
		let log = match proto.event {
			Some(pb::build_event::Event::Log(l)) => l,
			other => panic!("Expected Log variant, got {other:?}"),
		};
		assert_eq!(log.message, "Step 1/5: FROM rust:1.80");
		assert_eq!(log.timestamp, Some(to_proto_ts(ts)));
	}

	#[rstest]
	fn test_domain_event_to_proto_phase_change() {
		// Arrange
		let ts = fixed_timestamp();
		let event = BuildEvent::PhaseChange {
			phase: BuildPhase::Building,
			timestamp: ts,
		};

		// Act
		let proto = domain_event_to_proto(&event);

		// Assert
		let pc = match proto.event {
			Some(pb::build_event::Event::PhaseChange(p)) => p,
			other => panic!("Expected PhaseChange variant, got {other:?}"),
		};
		assert_eq!(pc.phase, pb::BuildPhase::Building as i32);
		assert_eq!(pc.timestamp, Some(to_proto_ts(ts)));
	}

	#[rstest]
	fn test_domain_event_to_proto_artifact_ready() {
		// Arrange
		let ts = fixed_timestamp();
		let event = BuildEvent::ArtifactReady {
			artifact_url: "registry.example.com/app:abc123".to_string(),
			digest: "sha256:deadbeef".to_string(),
			timestamp: ts,
		};

		// Act
		let proto = domain_event_to_proto(&event);

		// Assert
		let ar = match proto.event {
			Some(pb::build_event::Event::ArtifactReady(a)) => a,
			other => panic!("Expected ArtifactReady variant, got {other:?}"),
		};
		assert_eq!(ar.artifact_url, "registry.example.com/app:abc123");
		assert_eq!(ar.digest, "sha256:deadbeef");
		assert_eq!(ar.timestamp, Some(to_proto_ts(ts)));
	}

	#[rstest]
	fn test_domain_event_to_proto_error() {
		// Arrange
		let ts = fixed_timestamp();
		let event = BuildEvent::Error {
			message: "compilation failed".to_string(),
			timestamp: ts,
		};

		// Act
		let proto = domain_event_to_proto(&event);

		// Assert
		let e = match proto.event {
			Some(pb::build_event::Event::Error(e)) => e,
			other => panic!("Expected Error variant, got {other:?}"),
		};
		assert_eq!(e.message, "compilation failed");
		assert_eq!(e.timestamp, Some(to_proto_ts(ts)));
	}

	#[rstest]
	fn test_domain_event_to_proto_complete() {
		// Arrange
		let ts = fixed_timestamp();
		let event = BuildEvent::Complete {
			success: true,
			timestamp: ts,
		};

		// Act
		let proto = domain_event_to_proto(&event);

		// Assert
		let c = match proto.event {
			Some(pb::build_event::Event::Complete(c)) => c,
			other => panic!("Expected Complete variant, got {other:?}"),
		};
		assert!(c.success);
		assert_eq!(c.timestamp, Some(to_proto_ts(ts)));
	}

	// --- proto_event_to_domain: all 5 event variants (field-by-field) ---

	#[rstest]
	fn test_proto_event_to_domain_log() {
		// Arrange
		let ts = fixed_timestamp();
		let proto = pb::BuildEvent {
			event: Some(pb::build_event::Event::Log(pb::BuildLog {
				message: "building...".to_string(),
				timestamp: Some(to_proto_ts(ts)),
			})),
		};

		// Act
		let domain = proto_event_to_domain(&proto).unwrap();

		// Assert
		match domain {
			BuildEvent::Log { message, timestamp } => {
				assert_eq!(message, "building...");
				assert_eq!(timestamp, ts);
			}
			other => panic!("Expected Log variant, got {other:?}"),
		}
	}

	#[rstest]
	fn test_proto_event_to_domain_phase_change() {
		// Arrange
		let ts = fixed_timestamp();
		let proto = pb::BuildEvent {
			event: Some(pb::build_event::Event::PhaseChange(pb::BuildPhaseChange {
				phase: pb::BuildPhase::Pushing as i32,
				timestamp: Some(to_proto_ts(ts)),
			})),
		};

		// Act
		let domain = proto_event_to_domain(&proto).unwrap();

		// Assert
		match domain {
			BuildEvent::PhaseChange { phase, timestamp } => {
				assert_eq!(phase, BuildPhase::Pushing);
				assert_eq!(timestamp, ts);
			}
			other => panic!("Expected PhaseChange variant, got {other:?}"),
		}
	}

	#[rstest]
	fn test_proto_event_to_domain_artifact_ready() {
		// Arrange
		let ts = fixed_timestamp();
		let proto = pb::BuildEvent {
			event: Some(pb::build_event::Event::ArtifactReady(
				pb::BuildArtifactReady {
					artifact_url: "registry/app:v2".to_string(),
					digest: "sha256:aabb".to_string(),
					timestamp: Some(to_proto_ts(ts)),
				},
			)),
		};

		// Act
		let domain = proto_event_to_domain(&proto).unwrap();

		// Assert
		match domain {
			BuildEvent::ArtifactReady {
				artifact_url,
				digest,
				timestamp,
			} => {
				assert_eq!(artifact_url, "registry/app:v2");
				assert_eq!(digest, "sha256:aabb");
				assert_eq!(timestamp, ts);
			}
			other => panic!("Expected ArtifactReady variant, got {other:?}"),
		}
	}

	#[rstest]
	fn test_proto_event_to_domain_error() {
		// Arrange
		let ts = fixed_timestamp();
		let proto = pb::BuildEvent {
			event: Some(pb::build_event::Event::Error(pb::BuildError {
				message: "OOM killed".to_string(),
				timestamp: Some(to_proto_ts(ts)),
			})),
		};

		// Act
		let domain = proto_event_to_domain(&proto).unwrap();

		// Assert
		match domain {
			BuildEvent::Error { message, timestamp } => {
				assert_eq!(message, "OOM killed");
				assert_eq!(timestamp, ts);
			}
			other => panic!("Expected Error variant, got {other:?}"),
		}
	}

	#[rstest]
	fn test_proto_event_to_domain_complete() {
		// Arrange
		let ts = fixed_timestamp();
		let proto = pb::BuildEvent {
			event: Some(pb::build_event::Event::Complete(pb::BuildComplete {
				success: false,
				timestamp: Some(to_proto_ts(ts)),
			})),
		};

		// Act
		let domain = proto_event_to_domain(&proto).unwrap();

		// Assert
		match domain {
			BuildEvent::Complete { success, timestamp } => {
				assert!(!success);
				assert_eq!(timestamp, ts);
			}
			other => panic!("Expected Complete variant, got {other:?}"),
		}
	}

	// --- proto_event_to_domain with None event ---

	#[rstest]
	fn test_proto_event_to_domain_none_event() {
		// Arrange
		let proto = pb::BuildEvent { event: None };

		// Act
		let result = proto_event_to_domain(&proto);

		// Assert
		assert!(result.is_none());
	}

	// --- domain->proto->domain roundtrip for all 5 variants ---

	#[rstest]
	fn test_roundtrip_log() {
		// Arrange
		let ts = fixed_timestamp();
		let original = BuildEvent::Log {
			message: "roundtrip test".to_string(),
			timestamp: ts,
		};

		// Act
		let proto = domain_event_to_proto(&original);
		let roundtripped = proto_event_to_domain(&proto).unwrap();

		// Assert
		assert_eq!(roundtripped, original);
	}

	#[rstest]
	fn test_roundtrip_phase_change() {
		// Arrange
		let ts = fixed_timestamp();
		let original = BuildEvent::PhaseChange {
			phase: BuildPhase::Finalizing,
			timestamp: ts,
		};

		// Act
		let proto = domain_event_to_proto(&original);
		let roundtripped = proto_event_to_domain(&proto).unwrap();

		// Assert
		assert_eq!(roundtripped, original);
	}

	#[rstest]
	fn test_roundtrip_artifact_ready() {
		// Arrange
		let ts = fixed_timestamp();
		let original = BuildEvent::ArtifactReady {
			artifact_url: "registry.io/img:sha-abc".to_string(),
			digest: "sha256:1234".to_string(),
			timestamp: ts,
		};

		// Act
		let proto = domain_event_to_proto(&original);
		let roundtripped = proto_event_to_domain(&proto).unwrap();

		// Assert
		assert_eq!(roundtripped, original);
	}

	#[rstest]
	fn test_roundtrip_error() {
		// Arrange
		let ts = fixed_timestamp();
		let original = BuildEvent::Error {
			message: "disk full".to_string(),
			timestamp: ts,
		};

		// Act
		let proto = domain_event_to_proto(&original);
		let roundtripped = proto_event_to_domain(&proto).unwrap();

		// Assert
		assert_eq!(roundtripped, original);
	}

	#[rstest]
	fn test_roundtrip_complete() {
		// Arrange
		let ts = fixed_timestamp();
		let original = BuildEvent::Complete {
			success: true,
			timestamp: ts,
		};

		// Act
		let proto = domain_event_to_proto(&original);
		let roundtripped = proto_event_to_domain(&proto).unwrap();

		// Assert
		assert_eq!(roundtripped, original);
	}

	// --- proto_request_to_domain ---

	#[rstest]
	fn test_proto_request_to_domain_full_fields() {
		// Arrange
		let proto_req = pb::StartBuildRequest {
			app_name: "my-app".to_string(),
			image: "registry.io/my-app:v1".to_string(),
			env_vars: vec![
				pb::EnvVar {
					key: "NODE_ENV".to_string(),
					value: "production".to_string(),
				},
				pb::EnvVar {
					key: "PORT".to_string(),
					value: "8080".to_string(),
				},
			],
			dockerfile: Some("Dockerfile.prod".to_string()),
			context_path: Some("/src".to_string()),
		};

		// Act
		let domain = proto_request_to_domain(&proto_req);

		// Assert
		assert_eq!(domain.app_name, "my-app");
		assert_eq!(domain.image, "registry.io/my-app:v1");
		assert_eq!(domain.env_vars.len(), 2);
		assert_eq!(domain.env_vars[0].key, "NODE_ENV");
		assert_eq!(domain.env_vars[0].value, "production");
		assert_eq!(domain.env_vars[1].key, "PORT");
		assert_eq!(domain.env_vars[1].value, "8080");
		assert_eq!(domain.dockerfile, Some("Dockerfile.prod".to_string()));
		assert_eq!(domain.context_path, Some("/src".to_string()));
	}

	#[rstest]
	fn test_proto_request_to_domain_none_optional_fields() {
		// Arrange
		let proto_req = pb::StartBuildRequest {
			app_name: "minimal".to_string(),
			image: "img:latest".to_string(),
			env_vars: vec![],
			dockerfile: None,
			context_path: None,
		};

		// Act
		let domain = proto_request_to_domain(&proto_req);

		// Assert
		assert_eq!(domain.app_name, "minimal");
		assert_eq!(domain.image, "img:latest");
		assert!(domain.env_vars.is_empty());
		assert!(domain.dockerfile.is_none());
		assert!(domain.context_path.is_none());
	}

	// --- timestamp_from_chrono ---

	#[rstest]
	fn test_timestamp_from_chrono_epoch() {
		// Arrange
		let epoch = Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap();

		// Act
		let ts = timestamp_from_chrono(epoch).unwrap();

		// Assert
		assert_eq!(ts.seconds, 0);
		assert_eq!(ts.nanos, 0);
	}

	#[rstest]
	fn test_timestamp_from_chrono_with_nanos() {
		// Arrange
		let dt = Utc
			.with_ymd_and_hms(2025, 1, 1, 0, 0, 0)
			.unwrap()
			.checked_add_signed(chrono::Duration::nanoseconds(999_999_999))
			.unwrap();

		// Act
		let ts = timestamp_from_chrono(dt).unwrap();

		// Assert
		assert_eq!(ts.seconds, dt.timestamp());
		assert_eq!(ts.nanos, 999_999_999);
	}

	// --- proto_timestamp_to_chrono ---

	#[rstest]
	fn test_proto_timestamp_to_chrono_none_returns_now() {
		// Arrange — None timestamp

		// Act
		let before = Utc::now();
		let result = proto_timestamp_to_chrono(None);
		let after = Utc::now();

		// Assert — should be approximately "now"
		assert!(result >= before);
		assert!(result <= after);
	}

	#[rstest]
	fn test_proto_timestamp_to_chrono_valid() {
		// Arrange
		let ts = Timestamp {
			seconds: 1_700_000_000,
			nanos: 500_000_000,
		};

		// Act
		let result = proto_timestamp_to_chrono(Some(ts));

		// Assert
		assert_eq!(result.timestamp(), 1_700_000_000);
		assert_eq!(result.timestamp_subsec_nanos(), 500_000_000);
	}

	#[rstest]
	fn test_proto_timestamp_negative_nanos_clamped() {
		// Arrange — negative nanos triggers unwrap_or(0)
		let ts = Timestamp {
			seconds: 1_000_000,
			nanos: -1,
		};

		// Act
		let result = proto_timestamp_to_chrono(Some(ts));

		// Assert — nanos should be clamped to 0 via try_into().unwrap_or(0)
		assert_eq!(result.timestamp(), 1_000_000);
		assert_eq!(result.timestamp_subsec_nanos(), 0);
	}

	// --- domain_event with empty strings preserved ---

	#[rstest]
	fn test_domain_event_empty_strings_preserved() {
		// Arrange
		let ts = fixed_timestamp();
		let event = BuildEvent::Log {
			message: "".to_string(),
			timestamp: ts,
		};

		// Act
		let proto = domain_event_to_proto(&event);
		let roundtripped = proto_event_to_domain(&proto).unwrap();

		// Assert
		match roundtripped {
			BuildEvent::Log { message, .. } => assert_eq!(message, ""),
			other => panic!("Expected Log variant, got {other:?}"),
		}
	}

	#[rstest]
	fn test_domain_event_artifact_empty_strings() {
		// Arrange
		let ts = fixed_timestamp();
		let event = BuildEvent::ArtifactReady {
			artifact_url: "".to_string(),
			digest: "".to_string(),
			timestamp: ts,
		};

		// Act
		let proto = domain_event_to_proto(&event);
		let roundtripped = proto_event_to_domain(&proto).unwrap();

		// Assert
		match roundtripped {
			BuildEvent::ArtifactReady {
				artifact_url,
				digest,
				..
			} => {
				assert_eq!(artifact_url, "");
				assert_eq!(digest, "");
			}
			other => panic!("Expected ArtifactReady variant, got {other:?}"),
		}
	}
}
