//! Integration tests for Cluster Agent Service through gRPC.
//!
//! These tests verify the full roundtrip: domain service -> gRPC adapter ->
//! tonic server -> tonic client -> proto response, ensuring that
//! `MockClusterAgentService` and `AgentServiceGrpc` work correctly together.

use std::net::SocketAddr;
use std::sync::Arc;

use prost_types::Timestamp;
use reinhardt_cloud_core::mocks::MockClusterAgentService;
use reinhardt_cloud_grpc::services::cluster_agent::AgentServiceGrpc;
use reinhardt_cloud_proto::cluster_agent as pb;
use reinhardt_cloud_proto::cluster_agent::agent_service_client::AgentServiceClient;
use reinhardt_cloud_proto::cluster_agent::agent_service_server::AgentServiceServer;
use rstest::rstest;
use tonic::transport::{Channel, Server};
use uuid::Uuid;

/// Start a gRPC server with AgentService on a random port.
async fn start_agent_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
	let mock_service = Arc::new(MockClusterAgentService::new());
	let grpc_service = AgentServiceGrpc::new(mock_service);
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
	let addr = listener.local_addr().unwrap();

	let handle = tokio::spawn(async move {
		Server::builder()
			.add_service(AgentServiceServer::new(grpc_service))
			.serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
			.await
			.unwrap();
	});

	// Give server time to start accepting connections
	tokio::time::sleep(std::time::Duration::from_millis(100)).await;
	(addr, handle)
}

/// Connect an AgentService gRPC client to the given address.
async fn connect_agent_client(addr: SocketAddr) -> AgentServiceClient<Channel> {
	let endpoint = format!("http://{addr}");
	AgentServiceClient::connect(endpoint).await.unwrap()
}

#[rstest]
#[tokio::test]
async fn test_report_health_through_grpc() {
	// Arrange
	let (addr, _handle) = start_agent_server().await;
	let mut client = connect_agent_client(addr).await;

	let agent_id = Uuid::now_v7();
	let now = chrono::Utc::now();
	let health_report = pb::AgentHealthReport {
		agent_id: agent_id.to_string(),
		healthy: true,
		cpu_usage_percent: 35.5,
		memory_usage_percent: 62.0,
		pod_count: 12,
		reported_at: Some(Timestamp {
			seconds: now.timestamp(),
			nanos: now.timestamp_subsec_nanos() as i32,
		}),
	};

	// Act
	let response = client.report_health(health_report).await.unwrap();
	let status_response = response.into_inner();

	// Assert
	assert!(
		status_response.success,
		"Expected success=true from ReportHealth"
	);
	assert_eq!(
		status_response.message, "Health reported",
		"Expected message='Health reported'"
	);
}

#[rstest]
#[tokio::test]
async fn test_report_health_invalid_agent_id() {
	// Arrange
	let (addr, _handle) = start_agent_server().await;
	let mut client = connect_agent_client(addr).await;

	let health_report = pb::AgentHealthReport {
		agent_id: "not-a-uuid".to_string(),
		healthy: true,
		cpu_usage_percent: 10.0,
		memory_usage_percent: 20.0,
		pod_count: 3,
		reported_at: Some(Timestamp {
			seconds: chrono::Utc::now().timestamp(),
			nanos: 0,
		}),
	};

	// Act
	let result = client.report_health(health_report).await;

	// Assert -- should return INVALID_ARGUMENT for non-UUID agent_id
	assert!(result.is_err(), "Expected error for invalid agent_id");
	let status = result.unwrap_err();
	assert_eq!(
		status.code(),
		tonic::Code::InvalidArgument,
		"Expected INVALID_ARGUMENT status code, got {:?}",
		status.code()
	);
	assert!(
		status.message().contains("Invalid agent_id"),
		"Expected error message to mention 'Invalid agent_id', got: {}",
		status.message()
	);
}
