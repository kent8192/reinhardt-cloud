//! Configuration file handling for the reinhardt-cloud CLI.

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

/// Errors from configuration loading.
// allow(dead_code): Returned by from_file(); will be used when CLI loads
// reinhardt-cloud.toml configuration on startup.
#[allow(dead_code)]
#[derive(Debug, Error)]
pub(crate) enum ConfigError {
	#[error("failed to read config file: {0}")]
	ReadError(#[from] std::io::Error),

	#[error("failed to parse config: {0}")]
	ParseError(#[from] toml::de::Error),
}

/// CLI-specific configuration (read from `reinhardt-cloud.toml` or environment).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct CliConfig {
	/// API server base URL
	pub api_url: Option<String>,
	/// Application name
	pub app_name: Option<String>,
}

impl CliConfig {
	/// Loads configuration from a `reinhardt-cloud.toml` file.
	// allow(dead_code): Will be called when CLI implements config file loading.
	#[allow(dead_code)]
	pub(crate) fn from_file(path: &Path) -> Result<Self, ConfigError> {
		let content = std::fs::read_to_string(path)?;
		let config: CliConfig = toml::from_str(&content)?;
		Ok(config)
	}

	/// Returns the API URL, falling back to the REINHARDT_CLOUD_API_URL env var or default.
	pub(crate) fn api_url(&self) -> String {
		self.api_url
			.clone()
			.or_else(|| std::env::var("REINHARDT_CLOUD_API_URL").ok())
			.unwrap_or_else(|| "http://localhost:8000".to_string())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	use serial_test::serial;

	#[rstest]
	fn test_default_config_has_no_api_url() {
		// Arrange
		let config = CliConfig::default();

		// Act
		let api_url = &config.api_url;

		// Assert
		assert!(api_url.is_none());
	}

	#[rstest]
	fn test_default_config_has_no_app_name() {
		// Arrange
		let config = CliConfig::default();

		// Act
		let app_name = &config.app_name;

		// Assert
		assert!(app_name.is_none());
	}

	#[rstest]
	fn test_api_url_returns_configured_value() {
		// Arrange
		let config = CliConfig {
			api_url: Some("http://custom:9000".to_string()),
			app_name: None,
		};

		// Act
		let url = config.api_url();

		// Assert
		assert_eq!(url, "http://custom:9000");
	}

	#[rstest]
	#[serial(env)]
	fn test_api_url_falls_back_to_default() {
		// Arrange
		// Ensure env var is not set for this test
		// SAFETY: This test runs serially via #[serial(env)] and no other
		// thread depends on this env var during execution.
		unsafe {
			std::env::remove_var("REINHARDT_CLOUD_API_URL");
		}
		let config = CliConfig::default();

		// Act
		let url = config.api_url();

		// Assert
		assert_eq!(url, "http://localhost:8000");
	}

	#[rstest]
	fn test_from_file_returns_error_for_missing_file() {
		// Arrange
		let unique_name = format!(
			"nonexistent-reinhardt-cloud-config-{}.toml",
			std::process::id()
		);
		let path = std::env::temp_dir().join(unique_name);

		// Act
		let result = CliConfig::from_file(&path);

		// Assert
		assert!(result.is_err());
	}
}
