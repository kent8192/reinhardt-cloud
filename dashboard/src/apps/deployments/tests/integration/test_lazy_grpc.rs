//! Integration tests for the lazy gRPC channel singleton.
//!
//! These tests assert that:
//! 1. Constructing `GrpcChannelSingleton` against an unreachable operator
//!    endpoint does not fail at the transport layer (lazy connect).
//! 2. The gRPC `LogServiceClient` built from that channel surfaces
//!    `tonic::Status` with `Code::Unavailable` (or a similar transport
//!    error) when the operator is not listening, rather than panicking.
//!
//! Together these guarantee that the dashboard can boot and serve
//! non-log endpoints even when the operator's gRPC server is absent
//! — the motivation behind reinhardt-cloud#392.

#[cfg(test)]
mod tests {
	use reinhardt_cloud_proto::common::PaginationRequest;
	use reinhardt_cloud_proto::log as log_pb;
	use rstest::rstest;

	use crate::config::GrpcChannelSingleton;

	/// An unreachable endpoint used to simulate the operator being absent.
	///
	/// Port 9 is the TCP `discard` protocol; binding to it from user code
	/// is blocked on every mainstream OS, so a loopback connect is
	/// guaranteed to fail without any external infrastructure.
	const UNREACHABLE_ENDPOINT: &str = "http://127.0.0.1:9";

	#[rstest]
	#[tokio::test]
	async fn test_dashboard_boots_without_operator_reachable() {
		// Arrange — an endpoint that is guaranteed to refuse connections.
		let endpoint = UNREACHABLE_ENDPOINT;

		// Act — singleton construction must not attempt a TCP connect.
		let singleton = GrpcChannelSingleton::new(endpoint);

		// Assert — construction succeeds even though the endpoint is down.
		assert!(
			singleton.is_ok(),
			"GrpcChannelSingleton::new must be infallible for valid URIs \
			 regardless of endpoint reachability; got: {:?}",
			singleton.err()
		);
	}

	#[rstest]
	#[tokio::test]
	async fn test_rpc_returns_unavailable_when_operator_absent() {
		// Arrange — build a client on a lazy channel pointing at an
		// unreachable endpoint, then issue a no-op ListLogs RPC.
		let singleton = GrpcChannelSingleton::new(UNREACHABLE_ENDPOINT)
			.expect("lazy channel construction must succeed");
		let mut client =
			log_pb::log_service_client::LogServiceClient::new(singleton.channel.clone());
		let request = log_pb::ListLogsRequest {
			filter: Some(log_pb::LogFilter::default()),
			pagination: Some(PaginationRequest {
				page: 1,
				page_size: 1,
			}),
		};

		// Act — the RPC must fail at the transport layer, not panic.
		let result = client.list_logs(request).await;

		// Assert — error surfaces as a `tonic::Status`, which the view
		// layer maps to HTTP 503 via the normal error-translation path.
		let status = result.expect_err("RPC against unreachable operator must fail");
		assert_eq!(
			status.code(),
			tonic::Code::Unavailable,
			"expected Code::Unavailable for connection failures, got: {:?} ({})",
			status.code(),
			status.message()
		);
	}
}
