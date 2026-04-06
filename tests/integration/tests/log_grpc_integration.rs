//! Integration tests for Log Service through gRPC.
//!
//! These tests verify the full roundtrip: domain service -> gRPC adapter ->
//! tonic server -> tonic client -> proto response, ensuring that
//! `LocalLogService`, `LogBuffer`, and `LogServiceGrpc` work correctly together.

use std::net::SocketAddr;
use std::sync::Arc;

use chrono::Utc;
use reinhardt_cloud_core::services::log::buffer::LogBuffer;
use reinhardt_cloud_core::services::log::local::LocalLogService;
use reinhardt_cloud_core::traits::LogService;
use reinhardt_cloud_grpc::services::log::LogServiceGrpc;
use reinhardt_cloud_proto::common::PaginationRequest;
use reinhardt_cloud_proto::log::log_service_client::LogServiceClient;
use reinhardt_cloud_proto::log::log_service_server::LogServiceServer;
use reinhardt_cloud_proto::log::{self as pb};
use reinhardt_cloud_types::log::{LogEntry, LogLevel};
use rstest::rstest;
use tonic::transport::{Channel, Server};

/// Start a gRPC server with LogService on a random port.
/// Returns the address, background task handle, and the underlying domain
/// service so tests can push data directly.
async fn start_log_server(
    buffer: Arc<LogBuffer>,
) -> (
    SocketAddr,
    tokio::task::JoinHandle<()>,
    Arc<LocalLogService>,
) {
    let domain_service = Arc::new(LocalLogService::new(buffer));
    let grpc_service = LogServiceGrpc::new(domain_service.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        Server::builder()
            .add_service(LogServiceServer::new(grpc_service))
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    // Give server time to start accepting connections
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    (addr, handle, domain_service)
}

/// Connect a LogService gRPC client to the given address.
async fn connect_log_client(addr: SocketAddr) -> LogServiceClient<Channel> {
    let endpoint = format!("http://{addr}");
    LogServiceClient::connect(endpoint).await.unwrap()
}

/// Create a domain LogEntry for testing.
fn make_entry(source: &str, level: LogLevel, msg: &str) -> LogEntry {
    LogEntry {
        timestamp: Utc::now(),
        level,
        source: source.to_string(),
        message: msg.to_string(),
        metadata: None,
    }
}

#[rstest]
#[tokio::test]
async fn test_list_logs_through_grpc() {
    // Arrange -- push entries via the domain service, then query via gRPC
    let buffer = Arc::new(LogBuffer::new(1000));
    let (addr, _handle, domain_service) = start_log_server(buffer).await;

    // Push 5 log entries via the domain service
    let entries: Vec<LogEntry> = (0..5)
        .map(|i| make_entry("test-app", LogLevel::Info, &format!("log message {i}")))
        .collect();
    domain_service.push_logs(entries).await.unwrap();

    let mut client = connect_log_client(addr).await;

    // Act -- list logs via gRPC with pagination
    let response = client
        .list_logs(pb::ListLogsRequest {
            filter: None,
            pagination: Some(PaginationRequest {
                page: 1,
                page_size: 10,
            }),
        })
        .await
        .unwrap();
    let list_response = response.into_inner();

    // Assert
    assert_eq!(
        list_response.entries.len(),
        5,
        "Expected 5 log entries from ListLogs"
    );

    // Verify pagination metadata
    let pagination = list_response.pagination.unwrap();
    assert_eq!(pagination.total, 5, "Expected total=5");
    assert_eq!(pagination.page, 1, "Expected page=1");
    assert_eq!(pagination.page_size, 10, "Expected page_size=10");

    // Verify each entry has correct fields
    for (i, entry) in list_response.entries.iter().enumerate() {
        assert_eq!(entry.source, "test-app", "Entry {i}: source mismatch");
        assert_eq!(
            entry.level,
            pb::LogLevel::Info as i32,
            "Entry {i}: level mismatch"
        );
        assert!(
            entry.message.starts_with("log message"),
            "Entry {i}: message mismatch, got: {}",
            entry.message
        );
        assert!(
            entry.timestamp.is_some(),
            "Entry {i}: timestamp should be present"
        );
    }
}

#[rstest]
#[tokio::test]
async fn test_log_metadata_preserved_through_grpc() {
    // Arrange -- push an entry with complex JSON metadata
    let buffer = Arc::new(LogBuffer::new(1000));
    let (addr, _handle, domain_service) = start_log_server(buffer).await;

    let metadata = serde_json::json!({
        "request_id": "abc-123",
        "status_code": 200,
        "latency_ms": 42.5,
        "tags": ["api", "health"],
        "nested": {
            "key": "value"
        }
    });

    let entry = LogEntry {
        timestamp: Utc::now(),
        level: LogLevel::Warn,
        source: "api-gateway".to_string(),
        message: "Slow request detected".to_string(),
        metadata: Some(metadata.clone()),
    };
    domain_service.push_logs(vec![entry]).await.unwrap();

    let mut client = connect_log_client(addr).await;

    // Act -- list logs via gRPC
    let response = client
        .list_logs(pb::ListLogsRequest {
            filter: None,
            pagination: Some(PaginationRequest {
                page: 1,
                page_size: 10,
            }),
        })
        .await
        .unwrap();
    let list_response = response.into_inner();

    // Assert -- verify the entry and its metadata survived the proto roundtrip
    assert_eq!(list_response.entries.len(), 1, "Expected exactly 1 log entry");

    let proto_entry = &list_response.entries[0];
    assert_eq!(proto_entry.source, "api-gateway");
    assert_eq!(proto_entry.message, "Slow request detected");
    assert_eq!(
        proto_entry.level,
        pb::LogLevel::Warn as i32,
        "Expected Warn level"
    );

    // Verify metadata JSON survived the roundtrip
    assert!(
        proto_entry.metadata_json.is_some(),
        "metadata_json should be present"
    );
    let roundtripped_metadata: serde_json::Value =
        serde_json::from_str(proto_entry.metadata_json.as_ref().unwrap()).unwrap();

    assert_eq!(
        roundtripped_metadata["request_id"], "abc-123",
        "request_id should survive roundtrip"
    );
    assert_eq!(
        roundtripped_metadata["status_code"], 200,
        "status_code should survive roundtrip"
    );
    assert_eq!(
        roundtripped_metadata["latency_ms"], 42.5,
        "latency_ms should survive roundtrip"
    );
    assert_eq!(
        roundtripped_metadata["tags"],
        serde_json::json!(["api", "health"]),
        "tags array should survive roundtrip"
    );
    assert_eq!(
        roundtripped_metadata["nested"]["key"], "value",
        "nested object should survive roundtrip"
    );
}
