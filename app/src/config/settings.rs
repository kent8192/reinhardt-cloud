//! Settings module for Reinhardt Cloud
//!
//! This module provides environment-specific settings configuration using TOML files.
//!
//! ## Configuration Structure
//!
//! Settings are loaded from TOML files in the `settings/` directory:
//! - `base.toml` - Common settings across all environments
//! - `local.toml` - Local development settings
//! - `ci.toml` - CI (GitHub Actions) environment settings
//! - `staging.toml` - Staging environment settings
//! - `production.toml` - Production environment settings
//!
//! ## Priority Order
//!
//! Settings are merged with the following priority (highest to lowest):
//! 1. Environment-specific TOML file (e.g., `production.toml`)
//! 2. Base TOML file (`base.toml`)
//! 3. Environment variables with `REINHARDT_` prefix
//! 4. Default values
//!
//! ## Settings Directory Resolution
//!
//! The settings directory is resolved by:
//! 1. `REINHARDT_CLOUD_CONFIG_DIR` environment variable (for deployed environments)
//! 2. `CARGO_MANIFEST_DIR/settings` at compile time (for local development)
//!
//! ## Environment Selection
//!
//! The environment is determined by the `REINHARDT_ENV` environment variable:
//! - `local` or `development` → loads `local.toml`
//! - `ci` → loads `ci.toml`
//! - `staging` → loads `staging.toml`
//! - `production` → loads `production.toml`
//!
//! If `REINHARDT_ENV` is not set, it defaults to `local`.

// Allow deprecated Settings until migration to CoreSettings + ProjectSettings (#146)
#[allow(deprecated)]
use reinhardt::Settings;
use reinhardt::conf::settings::builder::SettingsBuilder;
use reinhardt::conf::settings::profile::Profile;
use reinhardt::conf::settings::sources::{DefaultSource, LowPriorityEnvSource, TomlFileSource};
use std::env;

/// Get settings based on environment variable
///
/// Reads the REINHARDT_ENV environment variable to determine which settings to load.
/// Defaults to "local" if not set.
///
/// # Examples
///
/// ```no_run
/// use reinhardt_cloud::config::settings::get_settings;
///
/// let settings = get_settings();
/// println!("Debug mode: {}", settings.debug);
/// ```
///
/// # Configuration Directory
///
/// The settings directory is resolved in the following order:
/// 1. `REINHARDT_CLOUD_CONFIG_DIR` environment variable (for deployed environments)
/// 2. `CARGO_MANIFEST_DIR/settings` at compile time (for local development)
///
/// # Panics
///
/// Panics if:
/// - Settings files cannot be read
/// - Settings cannot be deserialized
/// - Required settings are missing
#[allow(deprecated)]
pub fn get_settings() -> Settings {
	let profile_str = env::var("REINHARDT_ENV").unwrap_or_else(|_| "local".to_string());
	let profile = Profile::parse(&profile_str);

	// Resolve settings directory: REINHARDT_CLOUD_CONFIG_DIR env var takes precedence for deployed
	// environments (e.g., Docker, CI), falling back to compile-time CARGO_MANIFEST_DIR
	// for local development.
	let settings_dir = match env::var("REINHARDT_CLOUD_CONFIG_DIR") {
		Ok(dir) => std::path::PathBuf::from(dir),
		Err(_) => std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("settings"),
	};

	// Build settings by merging sources in priority order
	let merged = SettingsBuilder::new()
		.profile(profile)
		// Lowest priority: Default values
		.add_source(
			DefaultSource::new()
				.with_value(
					"base_dir",
					serde_json::Value::String(
						env::var("CARGO_MANIFEST_DIR")
							.unwrap_or_else(|_| ".".to_string()),
					),
				)
				.with_value(
					"secret_key",
					serde_json::Value::String("insecure-change-this-in-production".to_string()),
				)
				.with_value("debug", serde_json::Value::Bool(false))
				.with_value("allowed_hosts", serde_json::Value::Array(vec![]))
				.with_value("installed_apps", serde_json::Value::Array(vec![]))
				.with_value("middleware", serde_json::Value::Array(vec![]))
				.with_value("root_urlconf", serde_json::Value::String("".to_string()))
				.with_value("databases", serde_json::Value::Object(serde_json::Map::new()))
				.with_value("templates", serde_json::Value::Array(vec![]))
				.with_value("static_url", serde_json::Value::String("/static/".to_string()))
				.with_value("static_root", serde_json::Value::Null)
				.with_value("staticfiles_dirs", serde_json::Value::Array(vec![]))
				.with_value("media_url", serde_json::Value::String("/media/".to_string()))
				.with_value("media_root", serde_json::Value::Null)
				.with_value(
					"language_code",
					serde_json::Value::String("en-us".to_string()),
				)
				.with_value("time_zone", serde_json::Value::String("UTC".to_string()))
				.with_value("use_i18n", serde_json::Value::Bool(true))
				.with_value("use_tz", serde_json::Value::Bool(true))
				.with_value("append_slash", serde_json::Value::Bool(true))
				.with_value(
					"default_auto_field",
					serde_json::Value::String("BigAutoField".to_string()),
				)
				.with_value("secure_proxy_ssl_header", serde_json::Value::Null)
				.with_value("secure_ssl_redirect", serde_json::Value::Bool(false))
				.with_value("secure_hsts_seconds", serde_json::Value::Null)
				.with_value("secure_hsts_include_subdomains", serde_json::Value::Bool(false))
				.with_value("secure_hsts_preload", serde_json::Value::Bool(false))
				.with_value("session_cookie_secure", serde_json::Value::Bool(false))
				.with_value("csrf_cookie_secure", serde_json::Value::Bool(false))
				.with_value("admins", serde_json::Value::Array(vec![]))
				.with_value("managers", serde_json::Value::Array(vec![])),
		)
		// Low priority: Environment variables (for container overrides)
		.add_source(LowPriorityEnvSource::new().with_prefix("REINHARDT_"))
		// Medium priority: Base TOML file
		.add_source(TomlFileSource::new(settings_dir.join("base.toml")))
		// Highest priority: Environment-specific TOML file
		.add_source(TomlFileSource::new(
			settings_dir.join(format!("{}.toml", profile_str)),
		))
		.build()
		.expect("Failed to build settings");

	// Convert MergedSettings to reinhardt_core::Settings
	merged
		.into_typed()
		.expect("Failed to convert settings to Settings struct")
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_get_settings() {
		// Arrange / Act
		let settings = get_settings();

		// Assert
		assert!(!settings.core.secret_key.is_empty());
	}
}
