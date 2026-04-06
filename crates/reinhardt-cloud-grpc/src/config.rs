//! gRPC server configuration.

use std::time::Duration;

/// Configuration for the Reinhardt Cloud gRPC server.
#[derive(Debug, Clone)]
pub struct GrpcServerConfig {
	/// Port the gRPC server listens on.
	pub port: u16,
	/// Maximum message size in bytes (default: 4MB).
	pub max_message_size: usize,
	/// Request timeout duration.
	pub timeout: Duration,
	/// Optional TLS certificate path.
	pub tls_cert_path: Option<String>,
	/// Optional TLS key path.
	pub tls_key_path: Option<String>,
	/// Maximum concurrent connections.
	pub max_connections: Option<u32>,
}

impl Default for GrpcServerConfig {
	fn default() -> Self {
		Self {
			port: 50051,
			max_message_size: 4 * 1024 * 1024, // 4MB
			timeout: Duration::from_secs(30),
			tls_cert_path: None,
			tls_key_path: None,
			max_connections: None,
		}
	}
}

impl GrpcServerConfig {
	/// Returns the socket address string for binding.
	pub fn bind_address(&self) -> String {
		format!("0.0.0.0:{}", self.port)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_default_config() {
		// Act
		let config = GrpcServerConfig::default();

		// Assert
		assert_eq!(config.port, 50051);
		assert_eq!(config.max_message_size, 4 * 1024 * 1024);
		assert_eq!(config.timeout, Duration::from_secs(30));
		assert!(config.tls_cert_path.is_none());
		assert!(config.tls_key_path.is_none());
		assert!(config.max_connections.is_none());
	}

	#[rstest]
	fn test_bind_address() {
		// Arrange
		let config = GrpcServerConfig {
			port: 9090,
			..Default::default()
		};

		// Act
		let addr = config.bind_address();

		// Assert
		assert_eq!(addr, "0.0.0.0:9090");
	}
}
