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
use reinhardt::conf::settings::core_settings::{CoreSettings, HasCoreSettings};
use reinhardt::conf::settings::fragment::SettingsFragment;
use reinhardt::conf::settings::profile::Profile;
use reinhardt::conf::settings::sources::{DefaultSource, LowPriorityEnvSource, TomlFileSource};
// Workaround for kent8192/reinhardt-web#2846
// Remove this workaround when the upstream issue is resolved.
//
// Ideal implementation (without workaround):
//   use reinhardt::settings;
use reinhardt::macros::settings;
use std::env;
use std::path::PathBuf;

/// Composable project settings using the `#[settings]` macro.
///
/// Implicitly includes `CoreSettings` under the `core` field.
/// Additional fragments can be added as the project grows
/// (e.g., `cache: CacheSettings | session: SessionSettings`).
#[settings]
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
/// 2. Environment variables with `REINHARDT_` prefix
/// 3. Base TOML file (`base.toml`)
/// 4. Environment-specific TOML file (e.g., `local.toml`)
fn build_merged() -> reinhardt::conf::settings::builder::MergedSettings {
	let profile_str = profile_name();
	let profile = Profile::parse(&profile_str);
	let settings_dir = resolve_settings_dir();

	SettingsBuilder::new()
		.profile(profile)
		// Lowest priority: Default values (CoreSettings provides its own defaults via serde)
		.add_source(DefaultSource::new())
		// Low priority: Environment variables (for container/CI overrides)
		.add_source(LowPriorityEnvSource::new().with_prefix("REINHARDT_"))
		// Medium priority: Base TOML file
		.add_source(TomlFileSource::new(settings_dir.join("base.toml")))
		// Highest priority: Environment-specific TOML file
		.add_source(TomlFileSource::new(
			settings_dir.join(format!("{}.toml", profile_str)),
		))
		.build()
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
/// use reinhardt_cloud::config::settings::get_settings;
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
	build_merged()
		.into_typed()
		.expect("Failed to convert settings to ProjectSettings struct")
}

/// Get JWT secret from settings or environment.
///
/// Priority (highest to lowest):
/// 1. `REINHARDT_CLOUD_JWT_SECRET` environment variable
/// 2. `jwt_secret` key in the active TOML settings file
///
/// Returns `None` if the JWT secret is not configured in either source.
pub fn get_jwt_secret() -> Option<String> {
	// Env var takes highest priority (for container/CI overrides)
	if let Ok(secret) = env::var("REINHARDT_CLOUD_JWT_SECRET") {
		return Some(secret);
	}

	// Fall back to settings TOML
	build_merged()
		.get_raw("jwt_secret")
		.and_then(|v| v.as_str())
		.map(String::from)
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
