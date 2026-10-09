//! Integration tests that exercise the JWT interceptors through a real tonic
//! server.
//!
//! The interceptors decide which credential check applies from the request
//! path. tonic does not expose the path to a server-side `Interceptor` (the
//! `tonic::GrpcMethod` extension exists on clients only), so the path is
//! recorded by `GrpcPathService` ahead of `InterceptedService`. Unit tests
//! insert the path by hand, so only a real server proves the whole chain.
//! These tests wire the interceptors exactly as the dashboard does (user
//! interceptor on `BuildService`, method-aware interceptor on `LogService`,
//! agent interceptor on `AgentService`) and drive them with a real client.

use std::net::SocketAddr;
use std::sync::Arc;

use reinhardt_cloud_core::auth;
use reinhardt_cloud_core::mocks::{MockBuildService, MockClusterAgentService};
use reinhardt_cloud_core::services::log::{LocalLogService, LogBuffer};
use reinhardt_cloud_grpc::agent_claims::create_agent_token;
use reinhardt_cloud_grpc::interceptor::{
	AgentJwtInterceptor, JwtInterceptor, LogServiceJwtInterceptor, intercepted,
};
use reinhardt_cloud_grpc::services::build::BuildServiceGrpc;
use reinhardt_cloud_grpc::services::cluster_agent::AgentServiceGrpc;
use reinhardt_cloud_grpc::services::log::LogServiceGrpc;
use reinhardt_cloud_proto::build as build_pb;
use reinhardt_cloud_proto::build::build_service_client::BuildServiceClient;
use reinhardt_cloud_proto::build::build_service_server::BuildServiceServer;
use reinhardt_cloud_proto::cluster_agent as agent_pb;
use reinhardt_cloud_proto::cluster_agent::agent_service_client::AgentServiceClient;
use reinhardt_cloud_proto::cluster_agent::agent_service_server::AgentServiceServer;
use reinhardt_cloud_proto::common::PaginationRequest;
use reinhardt_cloud_proto::log as log_pb;
use reinhardt_cloud_proto::log::log_service_client::LogServiceClient;
use reinhardt_cloud_proto::log::log_service_server::LogServiceServer;
use rstest::{fixture, rstest};
use tonic::transport::{Channel, Server};
use tonic::{Code, Request, Status};
use uuid::Uuid;

const JWT_SECRET: &[u8] = b"interceptor-real-server-test-secret";

/// Cluster whose agent tokens the stateful validator reports as revoked.
const REVOKED_CLUSTER: Uuid = Uuid::from_u128(0x0000_0000_0000_7000_8000_0000_dead_beef);

/// A running tonic server; the serving task is aborted on drop.
struct TestServer {
	addr: SocketAddr,
	handle: tokio::task::JoinHandle<()>,
}

impl Drop for TestServer {
	fn drop(&mut self) {
		self.handle.abort();
	}
}

impl TestServer {
	async fn channel(&self) -> Channel {
		let endpoint = format!("http://{}", self.addr);
		// The listener is bound already; retry only covers the serving task not having polled yet.
		for _ in 0..40 {
			if let Ok(channel) = Channel::from_shared(endpoint.clone())
				.expect("valid endpoint")
				.connect()
				.await
			{
				return channel;
			}
			tokio::time::sleep(std::time::Duration::from_millis(50)).await;
		}
		panic!("could not connect to test gRPC server at {endpoint}");
	}
}

/// Stateful token check mirroring the dashboard's persisted-token validator.
// tonic interceptors propagate `tonic::Status` directly to clients, so the
// validator keeps the interceptor boundary unboxed (same as the dashboard).
#[allow(clippy::result_large_err)]
fn reject_revoked_cluster(
	_token: &str,
	claims: &reinhardt_cloud_grpc::agent_claims::AgentClaims,
) -> Result<(), Status> {
	if claims.cluster_id == REVOKED_CLUSTER.to_string() {
		return Err(Status::unauthenticated("Agent token has been revoked"));
	}
	Ok(())
}

/// Start a server wired like `dashboard/src/config/grpc.rs::start_grpc_server`.
#[fixture]
async fn server() -> TestServer {
	let user_interceptor = JwtInterceptor::new(JWT_SECRET);
	let log_interceptor = LogServiceJwtInterceptor::new(JWT_SECRET)
		.with_agent_token_validator(reject_revoked_cluster);
	let agent_interceptor =
		AgentJwtInterceptor::new(JWT_SECRET).with_token_validator(reject_revoked_cluster);

	let build_grpc = BuildServiceGrpc::new(Arc::new(MockBuildService::new()));
	let agent_grpc = AgentServiceGrpc::new(Arc::new(MockClusterAgentService::new()));
	let log_grpc = LogServiceGrpc::new(Arc::new(LocalLogService::new(Arc::new(LogBuffer::new(
		100,
	)))));

	let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
		.await
		.expect("bind ephemeral port");
	let addr = listener.local_addr().expect("local addr");

	let handle = tokio::spawn(async move {
		Server::builder()
			.add_service(intercepted(
				BuildServiceServer::new(build_grpc),
				user_interceptor,
			))
			.add_service(intercepted(
				LogServiceServer::new(log_grpc),
				log_interceptor,
			))
			.add_service(intercepted(
				AgentServiceServer::new(agent_grpc),
				agent_interceptor,
			))
			.serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
			.await
			.expect("test gRPC server failed");
	});

	TestServer { addr, handle }
}

/// RPCs under test, one per interceptor routing decision.
#[derive(Debug, Clone, Copy)]
enum Rpc {
	/// Non-agent path guarded by `JwtInterceptor`.
	BuildGetStatus,
	/// Non-push path guarded by the user branch of `LogServiceJwtInterceptor`.
	LogListLogs,
	/// `LogService/PushLogs`, guarded by the agent branch.
	LogPushLogs,
	/// Agent path guarded by `AgentJwtInterceptor`.
	AgentReportHealth,
}

/// Credentials a caller may present.
#[derive(Debug, Clone, Copy)]
enum Credential {
	Missing,
	ValidUser,
	ValidAgent,
	UserWrongSecret,
	UserExpired,
	AgentWrongSecret,
	AgentRevoked,
	NonBearerScheme,
	GarbageToken,
}

impl Credential {
	fn header_value(self) -> Option<String> {
		let user = |secret: &[u8], hours: i64| {
			auth::create_token(Uuid::now_v7(), "alice", secret, hours).expect("user token")
		};
		let agent = |cluster: Uuid, secret: &[u8]| {
			create_agent_token(cluster, secret, 1).expect("agent token")
		};
		match self {
			Self::Missing => None,
			Self::ValidUser => Some(format!("Bearer {}", user(JWT_SECRET, 1))),
			Self::ValidAgent => Some(format!("Bearer {}", agent(Uuid::now_v7(), JWT_SECRET))),
			Self::UserWrongSecret => Some(format!("Bearer {}", user(b"another-secret", 1))),
			Self::UserExpired => Some(format!("Bearer {}", user(JWT_SECRET, -1))),
			Self::AgentWrongSecret => Some(format!(
				"Bearer {}",
				agent(Uuid::now_v7(), b"another-secret")
			)),
			Self::AgentRevoked => Some(format!("Bearer {}", agent(REVOKED_CLUSTER, JWT_SECRET))),
			Self::NonBearerScheme => Some(format!("Token {}", user(JWT_SECRET, 1))),
			Self::GarbageToken => Some("Bearer not-a-jwt".to_string()),
		}
	}
}

fn request_with<T>(message: T, credential: Credential) -> Request<T> {
	let mut request = Request::new(message);
	if let Some(value) = credential.header_value() {
		request
			.metadata_mut()
			.insert("authorization", value.parse().expect("ascii header value"));
	}
	request
}

/// Call `rpc` with `credential` and return the outcome.
async fn call_status(server: &TestServer, rpc: Rpc, credential: Credential) -> Result<(), Status> {
	let channel = server.channel().await;
	match rpc {
		Rpc::BuildGetStatus => BuildServiceClient::new(channel)
			.get_build_status(request_with(
				build_pb::GetBuildStatusRequest {
					build_id: Uuid::now_v7().to_string(),
				},
				credential,
			))
			.await
			.map(|_| ()),
		Rpc::LogListLogs => LogServiceClient::new(channel)
			.list_logs(request_with(
				log_pb::ListLogsRequest {
					filter: Some(log_pb::LogFilter {
						source: Some("test-app".to_string()),
						..Default::default()
					}),
					pagination: Some(PaginationRequest {
						page: 1,
						page_size: 10,
					}),
				},
				credential,
			))
			.await
			.map(|_| ()),
		Rpc::LogPushLogs => {
			let batch = log_pb::PushLogsRequest { entries: vec![] };
			LogServiceClient::new(channel)
				.push_logs(request_with(tokio_stream::iter(vec![batch]), credential))
				.await
				.map(|_| ())
		}
		Rpc::AgentReportHealth => AgentServiceClient::new(channel)
			.report_health(request_with(
				agent_pb::AgentHealthReport {
					agent_id: Uuid::now_v7().to_string(),
					healthy: true,
					cpu_usage_percent: 1.0,
					memory_usage_percent: 1.0,
					pod_count: 1,
					reported_at: None,
				},
				credential,
			))
			.await
			.map(|_| ()),
	}
}

/// Call `rpc` with `credential` and return the resulting status code.
async fn call(server: &TestServer, rpc: Rpc, credential: Credential) -> Code {
	match call_status(server, rpc, credential).await {
		Ok(()) => Code::Ok,
		Err(status) => status.code(),
	}
}

// Unauthenticated calls and malformed credentials never reach the service.
#[rstest]
#[case::build_missing(Rpc::BuildGetStatus, Credential::Missing)]
#[case::build_wrong_secret(Rpc::BuildGetStatus, Credential::UserWrongSecret)]
#[case::build_expired(Rpc::BuildGetStatus, Credential::UserExpired)]
#[case::build_non_bearer(Rpc::BuildGetStatus, Credential::NonBearerScheme)]
#[case::build_garbage(Rpc::BuildGetStatus, Credential::GarbageToken)]
#[case::log_list_missing(Rpc::LogListLogs, Credential::Missing)]
#[case::log_list_garbage(Rpc::LogListLogs, Credential::GarbageToken)]
#[case::log_push_missing(Rpc::LogPushLogs, Credential::Missing)]
#[case::log_push_wrong_secret(Rpc::LogPushLogs, Credential::AgentWrongSecret)]
#[case::log_push_garbage(Rpc::LogPushLogs, Credential::GarbageToken)]
#[case::agent_missing(Rpc::AgentReportHealth, Credential::Missing)]
#[case::agent_wrong_secret(Rpc::AgentReportHealth, Credential::AgentWrongSecret)]
#[case::agent_garbage(Rpc::AgentReportHealth, Credential::GarbageToken)]
#[tokio::test]
async fn unauthenticated_call_is_rejected(
	#[future] server: TestServer,
	#[case] rpc: Rpc,
	#[case] credential: Credential,
) {
	// Arrange
	let server = server.await;

	// Act
	let code = call(&server, rpc, credential).await;

	// Assert
	assert_eq!(code, Code::Unauthenticated);
}

// A valid credential of the kind each path expects is accepted end to end.
#[rstest]
#[case::build_user(Rpc::BuildGetStatus, Credential::ValidUser)]
#[case::log_list_user(Rpc::LogListLogs, Credential::ValidUser)]
#[case::log_push_agent(Rpc::LogPushLogs, Credential::ValidAgent)]
#[case::agent_service_agent(Rpc::AgentReportHealth, Credential::ValidAgent)]
#[tokio::test]
async fn valid_credential_is_accepted(
	#[future] server: TestServer,
	#[case] rpc: Rpc,
	#[case] credential: Credential,
) {
	// Arrange
	let server = server.await;

	// Act
	let code = call(&server, rpc, credential).await;

	// Assert
	assert_eq!(code, Code::Ok);
}

// Agent paths and `LogService/PushLogs` demand an agent token, while every
// other path demands a user token; the wrong kind of token is refused.
#[rstest]
#[case::agent_service_rejects_user_token(Rpc::AgentReportHealth, Credential::ValidUser)]
#[case::push_logs_rejects_user_token(Rpc::LogPushLogs, Credential::ValidUser)]
#[case::list_logs_rejects_agent_token(Rpc::LogListLogs, Credential::ValidAgent)]
#[case::build_rejects_agent_token(Rpc::BuildGetStatus, Credential::ValidAgent)]
#[tokio::test]
async fn token_kind_must_match_path(
	#[future] server: TestServer,
	#[case] rpc: Rpc,
	#[case] credential: Credential,
) {
	// Arrange
	let server = server.await;

	// Act
	let code = call(&server, rpc, credential).await;

	// Assert
	assert_eq!(code, Code::Unauthenticated);
}

// The stateful validator runs for agent paths, including `PushLogs`.
#[rstest]
#[case::agent_service(Rpc::AgentReportHealth)]
#[case::push_logs(Rpc::LogPushLogs)]
#[tokio::test]
async fn revoked_agent_token_is_rejected(#[future] server: TestServer, #[case] rpc: Rpc) {
	// Arrange
	let server = server.await;

	// Act
	let code = call(&server, rpc, Credential::AgentRevoked).await;

	// Assert
	assert_eq!(code, Code::Unauthenticated);
}

// A rejected call must fail because of the credential, not because the
// interceptor lacks routing information. Asserting the exact message keeps
// the checks above from passing for the wrong reason.
#[rstest]
#[case::build_missing(Rpc::BuildGetStatus, "Missing authorization token")]
#[case::list_logs_missing(Rpc::LogListLogs, "Missing authorization token")]
#[case::push_logs_missing(Rpc::LogPushLogs, "Missing agent authorization token")]
#[case::agent_service_missing(Rpc::AgentReportHealth, "Missing agent authorization token")]
#[tokio::test]
async fn missing_credential_is_reported_as_missing_credential(
	#[future] server: TestServer,
	#[case] rpc: Rpc,
	#[case] expected_message: &str,
) {
	// Arrange
	let server = server.await;

	// Act
	let status = call_status(&server, rpc, Credential::Missing)
		.await
		.expect_err("a call without credentials must be rejected");

	// Assert
	assert_eq!(status.code(), Code::Unauthenticated);
	assert_eq!(status.message(), expected_message);
}

// Wiring an interceptor with `with_interceptor` alone (no `intercepted`)
// leaves the path unknown. The path-dependent interceptors must then fail
// closed, while the user check, which needs no path, keeps enforcing tokens.
#[rstest]
#[case::agent_service(
	Rpc::AgentReportHealth,
	Credential::ValidAgent,
	Some("Missing gRPC path; cannot authorize agent request")
)]
#[case::log_list(
	Rpc::LogListLogs,
	Credential::ValidUser,
	Some("Missing gRPC path; cannot authorize log service request")
)]
#[case::log_push(
	Rpc::LogPushLogs,
	Credential::ValidAgent,
	Some("Missing gRPC path; cannot authorize log service request")
)]
#[case::build_missing_token(
	Rpc::BuildGetStatus,
	Credential::Missing,
	Some("Missing authorization token")
)]
#[case::build_valid_user(Rpc::BuildGetStatus, Credential::ValidUser, None)]
#[tokio::test]
async fn interceptor_without_path_service_fails_closed(
	#[case] rpc: Rpc,
	#[case] credential: Credential,
	#[case] expected_rejection: Option<&str>,
) {
	// Arrange
	let build_grpc = BuildServiceGrpc::new(Arc::new(MockBuildService::new()));
	let agent_grpc = AgentServiceGrpc::new(Arc::new(MockClusterAgentService::new()));
	let log_grpc = LogServiceGrpc::new(Arc::new(LocalLogService::new(Arc::new(LogBuffer::new(
		100,
	)))));
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
		.await
		.expect("bind ephemeral port");
	let addr = listener.local_addr().expect("local addr");
	let handle = tokio::spawn(async move {
		Server::builder()
			.add_service(BuildServiceServer::with_interceptor(
				build_grpc,
				JwtInterceptor::new(JWT_SECRET),
			))
			.add_service(LogServiceServer::with_interceptor(
				log_grpc,
				LogServiceJwtInterceptor::new(JWT_SECRET),
			))
			.add_service(AgentServiceServer::with_interceptor(
				agent_grpc,
				AgentJwtInterceptor::new(JWT_SECRET),
			))
			.serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
			.await
			.expect("test gRPC server failed");
	});
	let server = TestServer { addr, handle };

	// Act
	let outcome = call_status(&server, rpc, credential).await;

	// Assert
	match (outcome, expected_rejection) {
		(Ok(()), None) => {}
		(Err(status), Some(message)) => {
			assert_eq!(status.code(), Code::Unauthenticated);
			assert_eq!(status.message(), message);
		}
		(outcome, expected) => panic!("unexpected outcome {outcome:?}, expected {expected:?}"),
	}
}
