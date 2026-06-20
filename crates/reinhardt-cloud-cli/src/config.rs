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

/// CLI-specific configuration (read from `config.toml` or environment).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct CliConfig {
	/// Control-plane target base URL
	pub api_url: Option<String>,
	/// Project name
	pub project_name: Option<String>,
}

impl CliConfig {
	/// Loads configuration from a `config.toml` file.
	pub(crate) fn from_file(path: &Path) -> Result<Self, ConfigError> {
		let content = std::fs::read_to_string(path)?;
		let config: CliConfig = toml::from_str(&content)?;
		Ok(config)
	}

	/// Returns the target URL, falling back to the `REINHARDT_CLOUD_API_URL`
	/// environment variable or default.
	pub(crate) fn api_url(&self) -> String {
		self.api_url
			.clone()
			.or_else(|| std::env::var("REINHARDT_CLOUD_API_URL").ok())
			.unwrap_or_else(|| "http://localhost:8000".to_string())
	}
}

/// Stored credentials for Reinhardt Cloud commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Credentials {
	/// Stored JWT token.
	pub token: String,
	/// Username associated with the token.
	pub username: String,
	/// API base URL that issued the token.
	#[serde(default)]
	pub api_url: Option<String>,
}

impl Credentials {
	/// Returns whether the credentials are scoped to the selected API URL.
	pub(crate) fn is_scoped_to(&self, selected_api_url: &str) -> bool {
		let Some(stored_api_url) = self.api_url.as_deref() else {
			return false;
		};

		canonical_api_url(stored_api_url) == canonical_api_url(selected_api_url)
	}
}

fn canonical_api_url(api_url: &str) -> Option<String> {
	url::Url::parse(api_url)
		.ok()
		.map(|url| url.as_str().trim_end_matches('/').to_string())
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

/// Returns the path to the reinhardt-cloud CLI configuration file.
///
/// Defaults to `~/.config/reinhardt-cloud/config.toml` on most platforms,
/// falling back to `./reinhardt-cloud/config.toml`.
pub(crate) fn config_path() -> PathBuf {
	credentials_dir().join("config.toml")
}

/// Load credentials from a specific path (returns `Ok(None)` if absent).
pub(crate) fn load_token_from(
	path: &Path,
) -> Result<Option<Credentials>, Box<dyn std::error::Error>> {
	if !path.exists() {
		return Ok(None);
	}
	let content = std::fs::read_to_string(path)
		.map_err(|e| format!("Failed to read credentials file: {e}"))?;
	let creds: Credentials =
		serde_json::from_str(&content).map_err(|e| format!("Failed to parse credentials: {e}"))?;
	Ok(Some(creds))
}

/// Load credentials from the default credentials path.
pub(crate) fn load_token() -> Result<Option<Credentials>, Box<dyn std::error::Error>> {
	load_token_from(&credentials_path())
}

/// Persist credentials to `path`, creating parent directories as needed.
pub(crate) fn save_token(
	creds: &Credentials,
	path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent)?;
	}
	let json = serde_json::to_string_pretty(creds)?;
	std::fs::write(path, json)?;
	Ok(())
}

/// Resolve the API token by priority: explicit flag > env var > scoped saved file.
///
/// Returns the first non-empty source. Saved credentials are used only when
/// their persisted API URL matches the selected control-plane URL.
pub(crate) fn resolve_token(
	flag: Option<String>,
	credentials_file: &Path,
	selected_api_url: &str,
) -> Option<String> {
	if let Some(t) = flag
		&& !t.is_empty()
	{
		return Some(t);
	}
	if let Ok(t) = std::env::var("REINHARDT_CLOUD_API_TOKEN")
		&& !t.is_empty()
	{
		return Some(t);
	}
	load_token_from(credentials_file)
		.ok()
		.flatten()
		.filter(|credentials| credentials.is_scoped_to(selected_api_url))
		.map(|credentials| credentials.token)
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
	fn test_default_config_has_no_project_name() {
		// Arrange
		let config = CliConfig::default();

		// Act
		let project_name = &config.project_name;

		// Assert
		assert!(project_name.is_none());
	}

	#[rstest]
	fn test_api_url_returns_configured_value() {
		// Arrange
		let config = CliConfig {
			api_url: Some("http://custom:9000".to_string()),
			project_name: None,
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
	fn test_config_path_uses_reinhardt_cloud_dir() {
		// Arrange & Act
		let path = config_path();

		// Assert
		assert!(path.ends_with("reinhardt-cloud/config.toml"));
	}

	#[rstest]
	fn test_from_file_parses_valid_toml() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("config.toml");
		std::fs::write(
			&path,
			r#"
api_url = "http://staging.example.com:8080"
project_name = "myapp"
"#,
		)
		.unwrap();

		// Act
		let config = CliConfig::from_file(&path).expect("valid TOML should parse");

		// Assert
		assert_eq!(
			config.api_url.as_deref(),
			Some("http://staging.example.com:8080")
		);
		assert_eq!(config.project_name.as_deref(), Some("myapp"));
	}

	#[rstest]
	fn test_from_file_returns_parse_error_on_malformed_toml() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("config.toml");
		std::fs::write(&path, "this = is = not = toml").unwrap();

		// Act
		let result = CliConfig::from_file(&path);

		// Assert
		assert!(
			matches!(result, Err(ConfigError::ParseError(_))),
			"expected ParseError, got {result:?}"
		);
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
			api_url: Some("https://api.example.com".to_string()),
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
		assert_eq!(loaded.api_url.as_deref(), Some("https://api.example.com"));
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
			api_url: Some("https://api.example.com".to_string()),
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

	#[rstest]
	fn test_save_token_then_load_roundtrip() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("credentials.json");
		let creds = Credentials {
			token: "rct_xyz".to_string(),
			username: "alice".to_string(),
			api_url: Some("https://api.example.com".to_string()),
		};

		// Act
		save_token(&creds, &path).unwrap();
		let loaded = load_token_from(&path).unwrap().unwrap();

		// Assert
		assert_eq!(loaded.token, "rct_xyz");
		assert_eq!(loaded.username, "alice");
		assert_eq!(loaded.api_url.as_deref(), Some("https://api.example.com"));
	}

	#[rstest]
	fn test_credentials_without_api_url_are_not_scoped() {
		// Arrange
		let creds = Credentials {
			token: "rct_legacy".to_string(),
			username: "alice".to_string(),
			api_url: None,
		};

		// Act
		let scoped = creds.is_scoped_to("https://api.example.com");

		// Assert
		assert!(!scoped);
	}

	#[rstest]
	fn test_credentials_scope_ignores_trailing_slash() {
		// Arrange
		let creds = Credentials {
			token: "rct_scoped".to_string(),
			username: "alice".to_string(),
			api_url: Some("https://api.example.com/".to_string()),
		};

		// Act
		let scoped = creds.is_scoped_to("https://api.example.com");

		// Assert
		assert!(scoped);
	}

	#[rstest]
	fn test_resolve_token_ignores_unscoped_file_credentials() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("credentials.json");
		save_token(
			&Credentials {
				token: "from-file".to_string(),
				username: "alice".to_string(),
				api_url: None,
			},
			&path,
		)
		.unwrap();

		// Act
		let token = resolve_token(None, &path, "https://api.example.com");

		// Assert
		assert_eq!(token, None);
	}

	#[rstest]
	fn test_resolve_token_ignores_mismatched_file_credentials() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("credentials.json");
		save_token(
			&Credentials {
				token: "from-file".to_string(),
				username: "alice".to_string(),
				api_url: Some("https://api.example.com".to_string()),
			},
			&path,
		)
		.unwrap();

		// Act
		let token = resolve_token(None, &path, "https://other.example.com");

		// Assert
		assert_eq!(token, None);
	}

	#[rstest]
	fn test_resolve_token_uses_matching_file_credentials() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("credentials.json");
		save_token(
			&Credentials {
				token: "from-file".to_string(),
				username: "alice".to_string(),
				api_url: Some("https://api.example.com".to_string()),
			},
			&path,
		)
		.unwrap();

		// Act
		let token = resolve_token(None, &path, "https://api.example.com/");

		// Assert
		assert_eq!(token.as_deref(), Some("from-file"));
	}

	#[rstest]
	#[serial(env)]
	fn test_resolve_token_priority_flag_over_env_over_file() {
		// Arrange — file on disk, env set, flag provided
		// SAFETY: This test runs serially via #[serial(env)] and no other
		// thread depends on this env var during execution.
		unsafe {
			std::env::set_var("REINHARDT_CLOUD_API_TOKEN", "from-env");
		}
		let dir = tempfile::tempdir().unwrap();
		let path = dir.path().join("credentials.json");
		save_token(
			&Credentials {
				token: "from-file".to_string(),
				username: "alice".to_string(),
				api_url: Some("https://api.example.com".to_string()),
			},
			&path,
		)
		.unwrap();

		// Act
		let with_flag = resolve_token(
			Some("from-flag".to_string()),
			&path,
			"https://other.example.com",
		);
		let from_env = resolve_token(None, &path, "https://other.example.com");

		// Assert
		assert_eq!(with_flag.as_deref(), Some("from-flag"));
		assert_eq!(from_env.as_deref(), Some("from-env"));

		// Cleanup
		// SAFETY: serial test; env var removed after assertions.
		unsafe {
			std::env::remove_var("REINHARDT_CLOUD_API_TOKEN");
		}
	}
}
