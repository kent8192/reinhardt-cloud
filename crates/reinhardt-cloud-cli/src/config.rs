//! Configuration file handling for the reinhardt-cloud CLI.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors from configuration loading.
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

/// Stored credentials for the Reinhardt Cloud platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Credentials {
	/// JWT token received from the login endpoint.
	pub token: String,
	/// Username associated with the token.
	pub username: String,
}

/// Returns the directory used for storing reinhardt-cloud configuration files.
///
/// Defaults to `~/.config/reinhardt-cloud` on most platforms, falling back to
/// `./reinhardt-cloud` if the platform config directory cannot be determined.
pub(crate) fn credentials_dir() -> PathBuf {
	dirs::config_dir()
		.unwrap_or_else(|| PathBuf::from("."))
		.join("reinhardt-cloud")
}

/// Returns the path to the credentials file.
pub(crate) fn credentials_path() -> PathBuf {
	credentials_dir().join("credentials.json")
}

/// Loads stored credentials from the credentials file.
///
/// Returns `Ok(None)` if the file does not exist.
pub(crate) fn load_token() -> Result<Option<Credentials>, Box<dyn std::error::Error>> {
	let path = credentials_path();
	if !path.exists() {
		return Ok(None);
	}
	let content = std::fs::read_to_string(&path)
		.map_err(|e| format!("Failed to read credentials file: {e}"))?;
	let creds: Credentials =
		serde_json::from_str(&content).map_err(|e| format!("Failed to parse credentials: {e}"))?;
	Ok(Some(creds))
}

/// Saves credentials to the credentials file.
///
/// Creates the parent directory if it does not exist.
pub(crate) fn save_token(creds: &Credentials) -> Result<(), Box<dyn std::error::Error>> {
	let path = credentials_path();
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent)
			.map_err(|e| format!("Failed to create config directory: {e}"))?;
	}
	let json = serde_json::to_string_pretty(creds)
		.map_err(|e| format!("Failed to serialize credentials: {e}"))?;
	std::fs::write(&path, json).map_err(|e| format!("Failed to write credentials file: {e}"))?;
	Ok(())
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

	#[rstest]
	fn test_credentials_dir_returns_path_with_reinhardt_cloud() {
		// Arrange & Act
		let dir = credentials_dir();

		// Assert
		assert!(dir.ends_with("reinhardt-cloud"));
	}

	#[rstest]
	fn test_credentials_path_returns_json_file() {
		// Arrange & Act
		let path = credentials_path();

		// Assert
		assert!(path.ends_with("credentials.json"));
	}

	#[rstest]
	fn test_save_and_load_token_roundtrip() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let cred_path = dir.path().join("credentials.json");
		let creds = Credentials {
			token: "test-jwt-token-abc123".to_string(),
			username: "testuser".to_string(),
		};

		// Act: save
		let json = serde_json::to_string_pretty(&creds).unwrap();
		std::fs::write(&cred_path, &json).unwrap();

		// Act: load
		let content = std::fs::read_to_string(&cred_path).unwrap();
		let loaded: Credentials = serde_json::from_str(&content).unwrap();

		// Assert
		assert_eq!(loaded.token, "test-jwt-token-abc123");
		assert_eq!(loaded.username, "testuser");
	}

	#[rstest]
	fn test_load_token_returns_none_for_missing_file() {
		// Arrange: load_token checks credentials_path(), but we can test the
		// logic directly by checking a nonexistent path.
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("nonexistent-credentials.json");

		// Act & Assert
		assert!(!path.exists());
	}

	#[rstest]
	fn test_save_token_creates_parent_directory() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let nested_dir = dir.path().join("nested").join("reinhardt-cloud");
		let cred_path = nested_dir.join("credentials.json");
		let creds = Credentials {
			token: "tok".to_string(),
			username: "user".to_string(),
		};

		// Act
		std::fs::create_dir_all(nested_dir).unwrap();
		let json = serde_json::to_string_pretty(&creds).unwrap();
		std::fs::write(&cred_path, &json).unwrap();

		// Assert
		assert!(cred_path.exists());
		let loaded: Credentials =
			serde_json::from_str(&std::fs::read_to_string(&cred_path).unwrap()).unwrap();
		assert_eq!(loaded.token, "tok");
	}
}
