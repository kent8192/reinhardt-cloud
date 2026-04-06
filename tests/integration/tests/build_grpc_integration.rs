//! Integration tests for Build Service through gRPC.
//!
//! These tests verify the full roundtrip: domain service -> gRPC adapter ->
//! tonic server -> tonic client -> proto response, ensuring that
//! `LocalBuildService` and `BuildServiceGrpc` work correctly together.

use std::net::SocketAddr;
use std::sync::Arc;

use reinhardt_cloud_core::services::build::local::LocalBuildService;
use reinhardt_cloud_grpc::services::build::BuildServiceGrpc;
use reinhardt_cloud_proto::build::build_service_client::BuildServiceClient;
use reinhardt_cloud_proto::build::build_service_server::BuildServiceServer;
use reinhardt_cloud_proto::build::{self as pb};
use rstest::rstest;
use tokio_stream::StreamExt;
use tonic::transport::{Channel, Server};

/// Start a gRPC server with BuildService on a random port and return the
/// address along with a handle to the background task.
async fn start_build_server(
    service: Arc<LocalBuildService>,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let grpc_service = BuildServiceGrpc::new(service);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        Server::builder()
            .add_service(BuildServiceServer::new(grpc_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    // Give server time to start accepting connections
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    (addr, handle)
}

/// Connect a BuildService gRPC client to the given address.
async fn connect_build_client(addr: SocketAddr) -> BuildServiceClient<Channel> {
    let endpoint = format!("http://{addr}");
    BuildServiceClient::connect(endpoint).await.unwrap()
}

#[rstest]
#[tokio::test]
async fn test_local_build_through_grpc_roundtrip() {
    // Arrange
    let service = Arc::new(LocalBuildService::new());
    let (addr, _handle) = start_build_server(service).await;
    let mut client = connect_build_client(addr).await;

    let request = pb::StartBuildRequest {
        app_name: "integration-app".to_string(),
        image: "registry.example.com/integration:v1".to_string(),
        env_vars: vec![],
        dockerfile: None,
        context_path: None,
    };

    // Act
    let response = client.start_build(request).await.unwrap();
    let mut stream = response.into_inner();
    let mut events = Vec::new();
    while let Some(result) = stream.next().await {
        events.push(result.unwrap());
    }

    // Assert -- should have phase changes, logs, artifact ready, and completion
    assert!(
        events.len() >= 6,
        "Expected at least 6 events (4 phases + logs + artifact + complete), got {}",
        events.len()
    );

    // Verify all 4 PhaseChange events are present
    let phase_changes: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.event, Some(pb::build_event::Event::PhaseChange(_))))
        .collect();
    assert_eq!(phase_changes.len(), 4, "Expected exactly 4 phase change events");

    // Verify an ArtifactReady event is present
    let artifact_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.event, Some(pb::build_event::Event::ArtifactReady(_))))
        .collect();
    assert_eq!(artifact_events.len(), 1, "Expected exactly 1 artifact ready event");

    // Verify the ArtifactReady event has non-empty fields
    if let Some(pb::build_event::Event::ArtifactReady(ar)) = &artifact_events[0].event {
        assert!(!ar.artifact_url.is_empty(), "artifact_url should not be empty");
        assert!(!ar.digest.is_empty(), "digest should not be empty");
        assert!(ar.digest.starts_with("sha256:"), "digest should start with sha256:");
    }

    // Verify the final event is Complete with success=true
    let last = events.last().unwrap();
    match &last.event {
        Some(pb::build_event::Event::Complete(c)) => {
            assert!(c.success, "Expected build to succeed");
            assert!(c.timestamp.is_some(), "Complete event should have a timestamp");
        }
        other => panic!("Expected Complete event as last event, got {other:?}"),
    }
}

#[rstest]
#[tokio::test]
async fn test_cancel_nonexistent_build_through_grpc() {
    // Arrange
    let service = Arc::new(LocalBuildService::new());
    let (addr, _handle) = start_build_server(service).await;
    let mut client = connect_build_client(addr).await;

    // Act -- CancelBuild with a valid UUID that does not correspond to any build
    let result = client
        .cancel_build(pb::CancelBuildRequest {
            build_id: uuid::Uuid::new_v4().to_string(),
        })
        .await;

    // Assert -- should fail because the build does not exist
    assert!(result.is_err(), "Expected error for non-existent build_id");
    let status = result.unwrap_err();
    assert_eq!(
        status.code(),
        tonic::Code::Internal,
        "Expected INTERNAL status code (mapped from NotFound), got {:?}",
        status.code()
    );
}

#[rstest]
#[tokio::test]
async fn test_get_build_status_unknown_build() {
    // Arrange
    let service = Arc::new(LocalBuildService::new());
    let (addr, _handle) = start_build_server(service).await;
    let mut client = connect_build_client(addr).await;

    // Act -- GetBuildStatus with a valid UUID that does not exist
    let result = client
        .get_build_status(pb::GetBuildStatusRequest {
            build_id: uuid::Uuid::new_v4().to_string(),
        })
        .await;

    // Assert -- should return NOT_FOUND
    assert!(result.is_err(), "Expected error for unknown build_id");
    let status = result.unwrap_err();
    assert_eq!(
        status.code(),
        tonic::Code::NotFound,
        "Expected NOT_FOUND status code, got {:?}",
        status.code()
    );
}

#[rstest]
#[tokio::test]
async fn test_build_client_invalid_build_id() {
    // Arrange
    let service = Arc::new(LocalBuildService::new());
    let (addr, _handle) = start_build_server(service).await;
    let mut client = connect_build_client(addr).await;

    // Act -- CancelBuild with an invalid (non-UUID) build_id
    let result = client
        .cancel_build(pb::CancelBuildRequest {
            build_id: "not-a-uuid".to_string(),
        })
        .await;

    // Assert -- should return INVALID_ARGUMENT
    assert!(result.is_err(), "Expected error for invalid build_id");
    let status = result.unwrap_err();
    assert_eq!(
        status.code(),
        tonic::Code::InvalidArgument,
        "Expected INVALID_ARGUMENT status code, got {:?}",
        status.code()
    );
    assert!(
        status.message().contains("Invalid build_id"),
        "Expected error message to mention 'Invalid build_id', got: {}",
        status.message()
    );
}
