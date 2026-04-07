//! gRPC server configuration and startup for Reinhardt Cloud.
//!
//! Launches a tonic gRPC server alongside the HTTP server, with health
//! checking and reflection services pre-registered.

use std::net::SocketAddr;
use std::sync::Arc;

use reinhardt_cloud_core::mocks::{MockBuildService, MockClusterAgentService};
use reinhardt_cloud_grpc::config::GrpcServerConfig;
use reinhardt_cloud_grpc::health;
use reinhardt_cloud_grpc::services::build::BuildServiceGrpc;
use reinhardt_cloud_grpc::services::cluster_agent::AgentServiceGrpc;
use reinhardt_cloud_proto::build::build_service_server::BuildServiceServer;
use reinhardt_cloud_proto::cluster_agent::agent_service_server::AgentServiceServer;
use tonic::transport::Server;
use tracing::info;

/// Mark a gRPC service as SERVING in the health reporter.
///
/// Centralises the health transition so each new service only needs a
/// single call instead of duplicated `mark_service_serving` lines.
async fn mark_service_healthy(health_reporter: &mut health::HealthReporter, service_name: &str) {
	health::mark_service_serving(health_reporter, service_name).await;
}

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

	// Register all known services as NOT_SERVING first, so that
	// services not explicitly marked as SERVING still appear in
	// health check responses.
	health::register_services(&mut health_reporter).await;

	// Create gRPC service instances backed by mock implementations
	let build_grpc = BuildServiceGrpc::new(Arc::new(MockBuildService::new()));
	let agent_grpc = AgentServiceGrpc::new(Arc::new(MockClusterAgentService::new()));

	// Mark active services as SERVING for health checks
	mark_service_healthy(&mut health_reporter, health::BUILD_SERVICE_NAME).await;
	mark_service_healthy(&mut health_reporter, health::AGENT_SERVICE_NAME).await;

	// Build reflection service from proto file descriptors
	let reflection_service = tonic_reflection::server::Builder::configure()
		.register_encoded_file_descriptor_set(reinhardt_cloud_proto::build::FILE_DESCRIPTOR_SET)
		.register_encoded_file_descriptor_set(
			reinhardt_cloud_proto::cluster_agent::FILE_DESCRIPTOR_SET,
		)
		.register_encoded_file_descriptor_set(reinhardt_cloud_proto::log::FILE_DESCRIPTOR_SET)
		.register_encoded_file_descriptor_set(reinhardt_cloud_proto::plugin::FILE_DESCRIPTOR_SET)
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
		.add_service(BuildServiceServer::new(build_grpc))
		.add_service(AgentServiceServer::new(agent_grpc))
		.serve_with_shutdown(addr, shutdown)
		.await
}
