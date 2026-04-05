//! gRPC server configuration and startup for Reinhardt Cloud.
//!
//! Launches a tonic gRPC server alongside the HTTP server, with health
//! checking and reflection services pre-registered.

use std::net::SocketAddr;

use reinhardt_cloud_grpc::config::GrpcServerConfig;
use reinhardt_cloud_grpc::health;
use tonic::transport::Server;
use tracing::info;

/// Build and start the gRPC server.
///
/// Registers health check and reflection services. The server
/// runs until the provided shutdown signal completes.
pub async fn start_grpc_server(
	config: GrpcServerConfig,
	shutdown: impl std::future::Future<Output = ()>,
) -> Result<(), tonic::transport::Error> {
	let addr: SocketAddr = config
		.bind_address()
		.parse()
		.expect("Invalid gRPC bind address");

	let (mut health_reporter, health_service) = health::create_health_service();
	health::register_services(&mut health_reporter).await;

	// Build reflection service from proto file descriptors
	let reflection_service = tonic_reflection::server::Builder::configure()
		.register_encoded_file_descriptor_set(reinhardt_cloud_proto::build::FILE_DESCRIPTOR_SET)
		.register_encoded_file_descriptor_set(
			reinhardt_cloud_proto::cluster_agent::FILE_DESCRIPTOR_SET,
		)
		.register_encoded_file_descriptor_set(reinhardt_cloud_proto::log::FILE_DESCRIPTOR_SET)
		.build_v1()
		.expect("Failed to build gRPC reflection service");

	info!(
		"Starting gRPC server on {} (max_message_size={}B, timeout={:?})",
		addr, config.max_message_size, config.timeout
	);

	Server::builder()
		.timeout(config.timeout)
		.add_service(health_service)
		.add_service(reflection_service)
		.serve_with_shutdown(addr, shutdown)
		.await
}
