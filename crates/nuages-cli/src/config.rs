//! Configuration file handling for the nuages CLI.

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

/// Errors from configuration loading.
#[derive(Debug, Error)]
pub(crate) enum ConfigError {
	#[error("failed to read config file: {0}")]
	ReadError(#[from] std::io::Error),

	#[error("failed to parse config: {0}")]
	ParseError(#[from] toml::de::Error),
}

/// CLI-specific configuration (read from reinhardt.toml or environment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct CliConfig {
	/// API server base URL
	pub api_url: Option<String>,
	/// Application name
	pub app_name: Option<String>,
}

impl CliConfig {
	/// Loads configuration from a reinhardt.toml file.
	pub(crate) fn from_file(path: &Path) -> Result<Self, ConfigError> {
		let content = std::fs::read_to_string(path)?;
		let config: CliConfig = toml::from_str(&content)?;
		Ok(config)
	}

	/// Returns the API URL, falling back to the NUAGES_API_URL env var or default.
	pub(crate) fn api_url(&self) -> String {
		self.api_url
			.clone()
			.or_else(|| std::env::var("NUAGES_API_URL").ok())
			.unwrap_or_else(|| "http://localhost:8000".to_string())
	}
}

impl Default for CliConfig {
	fn default() -> Self {
		Self {
			api_url: None,
			app_name: None,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

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
	fn test_api_url_falls_back_to_default() {
		// Arrange
		// Ensure env var is not set for this test
		// SAFETY: This test runs serially and no other thread depends on this env var.
		unsafe {
			std::env::remove_var("NUAGES_API_URL");
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
		let path = Path::new("/tmp/nonexistent-nuages-config.toml");

		// Act
		let result = CliConfig::from_file(path);

		// Assert
		assert!(result.is_err());
	}
}
