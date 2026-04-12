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

use reinhardt::conf::settings::builder::SettingsBuilder;
use reinhardt::conf::settings::profile::Profile;
use reinhardt::conf::settings::sources::{LowPriorityEnvSource, TomlFileSource};
use reinhardt::settings;
use std::env;
use std::path::PathBuf;

/// Composable project settings using the `#[settings]` macro.
///
/// Each fragment maps to a TOML section:
/// - `[core]` → `CoreSettings` (security fields nested under `[core.security]`)
/// - `[i18n]` → `I18nSettings`
/// - `[static_files]` → `StaticSettings`
/// - `[media]` → `MediaSettings`
/// - `[cors]` → `CorsSettings` (includes `allow_origins` used by `OriginGuardMiddleware`)
/// - `[email]` → `EmailSettings` (SMTP backend configuration for transactional emails)
#[settings(core: CoreSettings | I18nSettings | static_files: StaticSettings | MediaSettings | CorsSettings | EmailSettings)]
pub struct ProjectSettings;

/// Resolve the settings directory path.
///
/// `REINHARDT_CLOUD_CONFIG_DIR` env var takes precedence for deployed
/// environments (e.g., Docker, CI), falling back to compile-time
/// `CARGO_MANIFEST_DIR/settings` for local development.
fn resolve_settings_dir() -> PathBuf {
	match env::var("REINHARDT_CLOUD_CONFIG_DIR") {
		Ok(dir) => PathBuf::from(dir),
		Err(_) => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("settings"),
	}
}

/// Get the active environment profile name.
///
/// Reads `REINHARDT_ENV` and defaults to `"local"`.
fn profile_name() -> String {
	env::var("REINHARDT_ENV").unwrap_or_else(|_| "local".to_string())
}

/// Build merged settings from all configuration sources.
///
/// Sources are merged in priority order (lowest to highest):
/// 1. Default values (CoreSettings provides its own defaults via serde)
/// 2. Environment variables with `REINHARDT_` prefix (flat keys only)
/// 3. Base TOML file (`base.toml`)
/// 4. Environment-specific TOML file (e.g., `local.toml`)
fn build_settings() -> ProjectSettings {
	let profile_str = profile_name();
	let settings_dir = resolve_settings_dir();

	SettingsBuilder::new()
		.profile(Profile::parse(&profile_str))
		// Low priority: Environment variables (for container/CI overrides)
		.add_source(LowPriorityEnvSource::new().with_prefix("REINHARDT_"))
		// Medium priority: Base TOML file
		.add_source(TomlFileSource::new(settings_dir.join("base.toml")))
		// Highest priority: Environment-specific TOML file
		.add_source(TomlFileSource::new(
			settings_dir.join(format!("{}.toml", profile_str)),
		))
		.build_composed()
		.expect("Failed to build settings")
}

/// Get settings based on environment variable
///
/// Reads the REINHARDT_ENV environment variable to determine which settings to load.
/// Defaults to "local" if not set.
///
/// # Examples
///
/// ```no_run
/// use reinhardt_cloud_dashboard::config::settings::get_settings;
///
/// let settings = get_settings();
/// println!("Debug mode: {}", settings.core.debug);
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
pub fn get_settings() -> ProjectSettings {
	build_settings()
}

/// Get Redis URL from settings or environment.
///
/// Priority (highest to lowest):
/// 1. `redis_url` key in the active TOML settings file
/// 2. `REINHARDT_CLOUD_REDIS_URL` environment variable (fallback for CI/container overrides)
///
/// Returns `None` if the Redis URL is not configured in either source.
pub fn get_redis_url() -> Option<String> {
	// TOML settings take highest priority
	let profile_str = profile_name();
	let settings_dir = resolve_settings_dir();
	let from_toml = SettingsBuilder::new()
		.add_source(TomlFileSource::new(settings_dir.join("base.toml")))
		.add_source(TomlFileSource::new(
			settings_dir.join(format!("{}.toml", profile_str)),
		))
		.build()
		.ok()
		.and_then(|merged| {
			merged
				.get_raw("redis_url")
				.and_then(|v| v.as_str())
				.map(String::from)
		});

	if from_toml.is_some() {
		return from_toml;
	}

	// Fall back to env var for container/CI overrides
	env::var("REINHARDT_CLOUD_REDIS_URL").ok()
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

	#[rstest]
	fn test_core_settings_fields() {
		// Arrange / Act
		let settings = get_settings();

		// Assert
		assert!(settings.core.debug);
		assert!(!settings.core.allowed_hosts.is_empty());
		assert!(settings.core.databases.contains_key("default"));
	}

	#[rstest]
	fn test_core_database_config() {
		// Arrange / Act
		let settings = get_settings();
		let db = &settings.core.databases["default"];

		// Assert
		assert!(!db.engine.is_empty());
		assert!(!db.name.is_empty());
		assert!(db.host.is_some());
		assert!(db.port.is_some());
	}

	#[rstest]
	fn test_core_security_settings() {
		// Arrange / Act
		let settings = get_settings();

		// Assert — local profile has relaxed security
		assert!(!settings.core.security.secure_ssl_redirect);
		assert!(!settings.core.security.session_cookie_secure);
		assert!(!settings.core.security.csrf_cookie_secure);
		assert!(settings.core.security.append_slash);
	}

	#[rstest]
	fn test_i18n_settings() {
		// Arrange / Act
		let settings = get_settings();

		// Assert
		assert_eq!(settings.i18n.language_code, "en-us");
		assert_eq!(settings.i18n.time_zone, "UTC");
		assert!(settings.i18n.use_i18n);
		assert!(settings.i18n.use_tz);
	}

	#[rstest]
	fn test_static_files_settings() {
		// Arrange / Act
		let settings = get_settings();

		// Assert
		assert_eq!(settings.static_files.url, "/static/");
		assert!(!settings.static_files.root.as_os_str().is_empty());
	}

	#[rstest]
	fn test_media_settings() {
		// Arrange / Act
		let settings = get_settings();

		// Assert
		assert_eq!(settings.media.url, "/media/");
		assert!(!settings.media.root.as_os_str().is_empty());
	}
}
