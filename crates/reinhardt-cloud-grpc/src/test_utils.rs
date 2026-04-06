//! Test utilities for gRPC services.
//!
//! Provides an in-process gRPC test server with automatic port allocation
//! for use in integration tests.

use std::net::SocketAddr;

use tokio::net::TcpListener;
use tonic::transport::Server;
use tracing::info;

use crate::health;

/// An in-process gRPC test server with automatic port allocation.
///
/// Starts a tonic server on a random available port, registers health
/// services, and provides the address for client connections.
pub struct TestGrpcServer {
	addr: SocketAddr,
	shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
	handle: Option<tokio::task::JoinHandle<()>>,
}

impl TestGrpcServer {
	/// Start a new test gRPC server with health services.
	///
	/// Allocates a random port and starts serving immediately.
	pub async fn start() -> Self {
		let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
		let addr = listener.local_addr().unwrap();
		let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

		let (mut health_reporter, health_service) = health::create_health_service();
		health::register_services(&mut health_reporter).await;

		let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

		let handle = tokio::spawn(async move {
			info!("Test gRPC server started on {}", addr);
			Server::builder()
				.add_service(health_service)
				.serve_with_incoming_shutdown(incoming, async { drop(shutdown_rx.await) })
				.await
				.unwrap();
		});

		Self {
			addr,
			shutdown_tx: Some(shutdown_tx),
			handle: Some(handle),
		}
	}

	/// Get the address the server is listening on.
	pub fn addr(&self) -> SocketAddr {
		self.addr
	}

	/// Get the server endpoint URI for client connections.
	pub fn endpoint(&self) -> String {
		format!("http://{}", self.addr)
	}

	/// Shut down the test server gracefully.
	pub async fn shutdown(mut self) {
		if let Some(tx) = self.shutdown_tx.take() {
			let _ = tx.send(());
		}
		if let Some(handle) = self.handle.take() {
			let _ = handle.await;
		}
	}
}

impl Drop for TestGrpcServer {
	fn drop(&mut self) {
		// Best-effort shutdown if not explicitly called
		if let Some(tx) = self.shutdown_tx.take() {
			let _ = tx.send(());
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	use tonic::transport::Channel;
	use tonic_health::pb::HealthCheckRequest;
	use tonic_health::pb::health_client::HealthClient;

	#[rstest]
	#[tokio::test]
	async fn test_server_starts_and_responds_to_health_check() {
		// Arrange
		let server = TestGrpcServer::start().await;
		let channel = Channel::from_shared(server.endpoint())
			.unwrap()
			.connect()
			.await
			.unwrap();

		// Act
		let mut client = HealthClient::new(channel);
		let response = client
			.check(HealthCheckRequest {
				service: String::new(),
			})
			.await
			.unwrap();

		// Assert
		assert_eq!(
			response.into_inner().status(),
			tonic_health::pb::health_check_response::ServingStatus::Serving
		);

		// Cleanup
		server.shutdown().await;
	}

	#[rstest]
	#[tokio::test]
	async fn test_server_reports_build_service_not_serving() {
		// Arrange — TestGrpcServer registers health services but does not add
		// BuildService to the server, so health status should be NOT_SERVING.
		let server = TestGrpcServer::start().await;
		let channel = Channel::from_shared(server.endpoint())
			.unwrap()
			.connect()
			.await
			.unwrap();

		// Act
		let mut client = HealthClient::new(channel);
		let response = client
			.check(HealthCheckRequest {
				service: crate::health::BUILD_SERVICE_NAME.to_string(),
			})
			.await
			.unwrap();

		// Assert
		assert_eq!(
			response.into_inner().status(),
			tonic_health::pb::health_check_response::ServingStatus::NotServing
		);

		server.shutdown().await;
	}
}
