//! gRPC health checking and reflection services.

use tonic_health::pb::health_server::{Health, HealthServer};
pub use tonic_health::server::HealthReporter;
use tonic_health::server::health_reporter;

/// Service names for health checking registration.
pub const BUILD_SERVICE_NAME: &str = "reinhardt.cloud.build.BuildService";
pub const AGENT_SERVICE_NAME: &str = "reinhardt.cloud.cluster_agent.AgentService";
pub const LOG_SERVICE_NAME: &str = "reinhardt.cloud.log.LogService";

/// Create a health reporter and health service.
///
/// Returns a `(HealthReporter, HealthServer)` tuple. The reporter is used
/// to update service health status, and the server is added to the tonic
/// `Server` builder.
pub fn create_health_service() -> (HealthReporter, HealthServer<impl Health>) {
	health_reporter()
}

/// Register all gRPC services with initial NOT_SERVING status.
///
/// Services are registered as NOT_SERVING by default. Call
/// [`mark_service_serving`] once a service is actually added to the
/// tonic `Server` builder so health checks accurately reflect availability.
pub async fn register_services(reporter: &mut HealthReporter) {
	reporter
		.set_service_status(BUILD_SERVICE_NAME, tonic_health::ServingStatus::NotServing)
		.await;
	reporter
		.set_service_status(AGENT_SERVICE_NAME, tonic_health::ServingStatus::NotServing)
		.await;
	reporter
		.set_service_status(LOG_SERVICE_NAME, tonic_health::ServingStatus::NotServing)
		.await;
}

/// Mark a single service as SERVING after it has been registered on the server.
pub async fn mark_service_serving(reporter: &mut HealthReporter, service_name: &str) {
	reporter
		.set_service_status(service_name, tonic_health::ServingStatus::Serving)
		.await;
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_service_names() {
		// Assert — verify service names match proto package paths
		assert!(BUILD_SERVICE_NAME.starts_with("reinhardt.cloud.build"));
		assert!(AGENT_SERVICE_NAME.starts_with("reinhardt.cloud.cluster_agent"));
		assert!(LOG_SERVICE_NAME.starts_with("reinhardt.cloud.log"));
	}

	#[rstest]
	#[tokio::test]
	async fn test_create_health_service() {
		// Act
		let (mut reporter, _service) = create_health_service();

		// Assert — registering services should not panic
		register_services(&mut reporter).await;
	}
}
