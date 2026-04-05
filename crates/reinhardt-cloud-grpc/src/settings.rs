//! gRPC settings deserialization from TOML configuration.

use std::time::Duration;

use serde::Deserialize;

use crate::config::GrpcServerConfig;

/// TOML-deserializable gRPC settings.
///
/// Maps to the `[grpc]` section in `reinhardt-cloud.toml`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct GrpcSettings {
	/// gRPC server port (default: 50051).
	pub port: u16,
	/// Maximum message size in bytes (default: 4194304 = 4MB).
	pub max_message_size: usize,
	/// Request timeout in seconds (default: 30).
	pub timeout_secs: u64,
	/// Path to TLS certificate file.
	pub tls_cert_path: Option<String>,
	/// Path to TLS private key file.
	pub tls_key_path: Option<String>,
	/// Maximum concurrent connections.
	pub max_connections: Option<u32>,
}

impl Default for GrpcSettings {
	fn default() -> Self {
		Self {
			port: 50051,
			max_message_size: 4 * 1024 * 1024,
			timeout_secs: 30,
			tls_cert_path: None,
			tls_key_path: None,
			max_connections: None,
		}
	}
}

impl From<GrpcSettings> for GrpcServerConfig {
	fn from(settings: GrpcSettings) -> Self {
		Self {
			port: settings.port,
			max_message_size: settings.max_message_size,
			timeout: Duration::from_secs(settings.timeout_secs),
			tls_cert_path: settings.tls_cert_path,
			tls_key_path: settings.tls_key_path,
			max_connections: settings.max_connections,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_default_settings() {
		// Act
		let settings = GrpcSettings::default();

		// Assert
		assert_eq!(settings.port, 50051);
		assert_eq!(settings.max_message_size, 4 * 1024 * 1024);
		assert_eq!(settings.timeout_secs, 30);
	}

	#[rstest]
	fn test_settings_from_toml() {
		// Arrange
		let toml_str = r#"
			port = 9090
			max_message_size = 8388608
			timeout_secs = 60
			tls_cert_path = "/etc/ssl/cert.pem"
			tls_key_path = "/etc/ssl/key.pem"
			max_connections = 1000
		"#;

		// Act
		let settings: GrpcSettings = toml::from_str(toml_str).unwrap();

		// Assert
		assert_eq!(settings.port, 9090);
		assert_eq!(settings.max_message_size, 8388608);
		assert_eq!(settings.timeout_secs, 60);
		assert_eq!(
			settings.tls_cert_path,
			Some("/etc/ssl/cert.pem".to_string())
		);
		assert_eq!(settings.max_connections, Some(1000));
	}

	#[rstest]
	fn test_settings_to_config_conversion() {
		// Arrange
		let settings = GrpcSettings {
			port: 8080,
			max_message_size: 2 * 1024 * 1024,
			timeout_secs: 15,
			tls_cert_path: None,
			tls_key_path: None,
			max_connections: Some(500),
		};

		// Act
		let config: GrpcServerConfig = settings.into();

		// Assert
		assert_eq!(config.port, 8080);
		assert_eq!(config.max_message_size, 2 * 1024 * 1024);
		assert_eq!(config.timeout, Duration::from_secs(15));
		assert_eq!(config.max_connections, Some(500));
	}

	#[rstest]
	fn test_partial_toml_uses_defaults() {
		// Arrange
		let toml_str = r#"port = 7777"#;

		// Act
		let settings: GrpcSettings = toml::from_str(toml_str).unwrap();

		// Assert
		assert_eq!(settings.port, 7777);
		assert_eq!(settings.max_message_size, 4 * 1024 * 1024); // default
		assert_eq!(settings.timeout_secs, 30); // default
	}
}
