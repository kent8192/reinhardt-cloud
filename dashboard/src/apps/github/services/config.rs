//! GitHub App settings.
//!
//! These values are process configuration and secrets. Installation access
//! tokens are not stored here or in the database; they are minted just in time.

use std::env;
use std::fmt;

use reinhardt::di::FactoryOutput;

const APP_ID_ENV: &str = "REINHARDT_CLOUD_GITHUB_APP_ID";
const PRIVATE_KEY_PEM_ENV: &str = "REINHARDT_CLOUD_GITHUB_APP_PRIVATE_KEY_PEM";
const WEBHOOK_SECRET_ENV: &str = "REINHARDT_CLOUD_GITHUB_WEBHOOK_SECRET";
const API_BASE_URL_ENV: &str = "REINHARDT_CLOUD_GITHUB_API_BASE_URL";
const INSTALL_URL_ENV: &str = "REINHARDT_CLOUD_GITHUB_APP_INSTALL_URL";
const DEFAULT_API_BASE_URL: &str = "https://api.github.com";

/// Runtime settings required to operate as a GitHub App.
#[derive(Clone, PartialEq, Eq)]
pub struct GitHubAppSettings {
	pub app_id: i64,
	pub private_key_pem: String,
	pub webhook_secret: String,
	pub api_base_url: String,
	pub install_url: Option<String>,
}

#[reinhardt::di::injectable_key]
pub struct GitHubAppSettingsKey;

/// Error returned when GitHub App settings cannot be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubAppSettingsError {
	message: String,
}

impl GitHubAppSettings {
	/// Reads GitHub App settings from process environment variables.
	pub fn from_env() -> Result<Self, GitHubAppSettingsError> {
		let app_id = required_env(APP_ID_ENV)?.parse::<i64>().map_err(|_| {
			GitHubAppSettingsError::new(format!("{APP_ID_ENV} must be a valid i64"))
		})?;
		let private_key_pem = required_private_key_pem()?;
		let webhook_secret = required_env(WEBHOOK_SECRET_ENV)?;
		let api_base_url =
			optional_env(API_BASE_URL_ENV).unwrap_or_else(|| DEFAULT_API_BASE_URL.to_string());
		let install_url = optional_env(INSTALL_URL_ENV);

		Ok(Self {
			app_id,
			private_key_pem,
			webhook_secret,
			api_base_url,
			install_url,
		})
	}

	/// Reads the configured GitHub App installation URL, if present.
	pub fn install_url_from_env() -> Option<String> {
		optional_env(INSTALL_URL_ENV)
	}
}

impl fmt::Debug for GitHubAppSettings {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("GitHubAppSettings")
			.field("app_id", &self.app_id)
			.field("private_key_pem", &"[redacted]")
			.field("webhook_secret", &"[redacted]")
			.field("api_base_url", &self.api_base_url)
			.field("install_url", &self.install_url)
			.finish()
	}
}

impl GitHubAppSettingsError {
	fn new(message: String) -> Self {
		Self { message }
	}
}

impl fmt::Display for GitHubAppSettingsError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str(&self.message)
	}
}

impl std::error::Error for GitHubAppSettingsError {}

/// DI factory for GitHub App settings.
#[reinhardt::di::injectable(scope = "singleton")]
async fn create_github_app_settings() -> FactoryOutput<GitHubAppSettingsKey, GitHubAppSettings> {
	FactoryOutput::new(
		GitHubAppSettings::from_env().expect("GitHub App settings env vars must be configured"),
	)
}

fn required_env(key: &str) -> Result<String, GitHubAppSettingsError> {
	optional_env(key).ok_or_else(|| GitHubAppSettingsError::new(format!("{key} is required")))
}

fn required_private_key_pem() -> Result<String, GitHubAppSettingsError> {
	let private_key_pem = required_env(PRIVATE_KEY_PEM_ENV)?.replace("\\n", "\n");
	if private_key_pem.trim().is_empty() {
		return Err(GitHubAppSettingsError::new(format!(
			"{PRIVATE_KEY_PEM_ENV} is required"
		)));
	}
	Ok(private_key_pem)
}

fn optional_env(key: &str) -> Option<String> {
	env::var(key).ok().filter(|value| !value.trim().is_empty())
}
