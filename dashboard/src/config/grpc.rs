//! gRPC server configuration and startup for Reinhardt Cloud.
//!
//! Launches a tonic gRPC server alongside the HTTP server, with health
//! checking and reflection services pre-registered.

use std::net::SocketAddr;
use std::sync::Arc;

use reinhardt::di::{FactoryOutput, InjectionContext};
use reinhardt_cloud_core::mocks::MockBuildService;
use reinhardt_cloud_grpc::config::GrpcServerConfig;
use reinhardt_cloud_grpc::health;
use reinhardt_cloud_grpc::interceptor::AgentJwtInterceptor;
use reinhardt_cloud_grpc::registry::AgentRegistry;
use reinhardt_cloud_grpc::services::build::BuildServiceGrpc;
use reinhardt_cloud_grpc::services::cluster_agent::{AgentServiceGrpc, RegistryBackedAgentService};
use reinhardt_cloud_grpc::services::log::LogServiceGrpc;
use reinhardt_cloud_proto::build::build_service_server::BuildServiceServer;
use reinhardt_cloud_proto::cluster_agent::agent_service_server::AgentServiceServer;
use reinhardt_cloud_proto::log::log_service_server::LogServiceServer;
use tonic::transport::Server;
use tracing::info;

use crate::apps::clusters::services::{JwtSecret, JwtSecretKey};
use crate::config::settings::{LogBackend, get_log_backend, get_loki_endpoint};

#[derive(Clone)]
pub struct AgentRegistrySingleton(pub Arc<AgentRegistry>);

#[reinhardt::di::injectable_key]
pub struct AgentRegistrySingletonKey;

#[reinhardt::di::injectable(scope = "singleton")]
async fn create_agent_registry_singleton()
-> FactoryOutput<AgentRegistrySingletonKey, AgentRegistrySingleton> {
	FactoryOutput::new(AgentRegistrySingleton(Arc::new(AgentRegistry::new())))
}

/// Wrapper holding the resolved log backend (`Arc<dyn LogService>`) injected
/// into the gRPC `LogServiceServer`.
#[derive(Clone)]
pub struct LogServiceSingleton(pub Arc<dyn reinhardt_cloud_core::traits::LogService>);

#[reinhardt::di::injectable_key]
pub struct LogServiceSingletonKey;

/// Build the configured log backend from settings.
///
/// `Memory` (default) serves an in-process ring buffer (`LocalLogService`); `Loki`
/// routes reads to the configured Loki endpoint via the read-oriented
/// `LokiLogService`. Constructed once as a singleton so the TLS/connection
/// pool cost is paid only at startup.
fn build_log_service() -> Arc<dyn reinhardt_cloud_core::traits::LogService> {
	use reinhardt_cloud_core::services::log::{LocalLogService, LogBuffer};
	use reinhardt_cloud_telemetry::LokiLogService;

	match get_log_backend() {
		LogBackend::Memory => Arc::new(LocalLogService::new(Arc::new(LogBuffer::new(1000)))),
		LogBackend::Loki => Arc::new(LokiLogService::new(get_loki_endpoint())),
	}
}

#[reinhardt::di::injectable(scope = "singleton")]
async fn create_log_service_singleton() -> FactoryOutput<LogServiceSingletonKey, LogServiceSingleton>
{
	FactoryOutput::new(LogServiceSingleton(build_log_service()))
}

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
///
/// `di_context` must be the root `InjectionContext` resolved at runserver
/// startup (typically `RunserverContext::di_context`). Using the caller-
/// supplied context avoids relying on the task-local resolve scope, which
/// is only set inside `#[injectable]` execution
/// and is therefore absent in spawned runserver-hook tasks.
pub async fn start_grpc_server(
	config: GrpcServerConfig,
	di_context: Arc<InjectionContext>,
	shutdown: impl std::future::Future<Output = ()>,
) -> Result<(), tonic::transport::Error> {
	let addr: SocketAddr = config
		.bind_address()
		.parse()
		.expect("Invalid gRPC bind address");

	// Resolve the JWT secret once at startup so a missing
	// `REINHARDT_CLOUD_JWT_SECRET` fails fast instead of letting
	// agents connect to an unauthenticated server.
	let jwt_secret = di_context
		.resolve::<FactoryOutput<JwtSecretKey, JwtSecret>>()
		.await
		.expect("Cannot start gRPC server without REINHARDT_CLOUD_JWT_SECRET");
	let agent_interceptor = AgentJwtInterceptor::new(jwt_secret.0.as_bytes());

	let (mut health_reporter, health_service) = health::create_health_service();

	// Register all known services as NOT_SERVING first, so that
	// services not explicitly marked as SERVING still appear in
	// health check responses.
	health::register_services(&mut health_reporter).await;

	// Create gRPC service instances. BuildService delegates to
	// MockBuildService while the build pipeline is wired up.
	// ClusterAgentService is backed by the real AgentRegistry so
	// Deploy/Rollback/Scale/Restart commands route to the right
	// agent by cluster_id.
	let build_grpc = BuildServiceGrpc::new(Arc::new(MockBuildService::new()));
	let agent_registry = di_context
		.resolve::<FactoryOutput<AgentRegistrySingletonKey, AgentRegistrySingleton>>()
		.await
		.expect("Cannot start gRPC server without AgentRegistrySingleton")
		.0
		.clone();
	let agent_grpc =
		AgentServiceGrpc::new(Arc::new(RegistryBackedAgentService::new(agent_registry)));

	// Resolve the configured log backend (in-memory or Loki) and wrap it in the
	// gRPC LogService. The dashboard server functions and the WebSocket
	// consumer both call this server.
	let log_service = di_context
		.resolve::<FactoryOutput<LogServiceSingletonKey, LogServiceSingleton>>()
		.await
		.expect("Cannot start gRPC server without LogServiceSingleton")
		.0
		.clone();
	let log_grpc = LogServiceGrpc::new(log_service);

	// Mark active services as SERVING for health checks
	mark_service_healthy(&mut health_reporter, health::BUILD_SERVICE_NAME).await;
	mark_service_healthy(&mut health_reporter, health::AGENT_SERVICE_NAME).await;
	mark_service_healthy(&mut health_reporter, health::LOG_SERVICE_NAME).await;

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
		.add_service(LogServiceServer::new(log_grpc))
		// AgentJwtInterceptor verifies the agent JWT and injects
		// `AgentClaims` into request extensions so downstream service
		// methods can route by the authenticated `cluster_id`.
		.add_service(AgentServiceServer::with_interceptor(
			agent_grpc,
			agent_interceptor,
		))
		.serve_with_shutdown(addr, shutdown)
		.await
}
