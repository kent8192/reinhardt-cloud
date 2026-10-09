//! Settings module for cloud_control_plane
//!
//! This module provides environment-specific settings configuration using TOML files.
//!
//! ## Configuration Structure
//!
//! Settings are loaded from TOML files in the `settings/` directory:
//! - `base.toml` - Common settings across all environments
//! - `local.toml` - Local development settings
//! - `staging.toml` - Staging environment settings
//! - `production.toml` - Production environment settings
//!
//! ## Priority Order
//!
//! Settings are merged with the following priority (highest to lowest):
//! 1. Environment variables with `REINHARDT_` prefix
//! 2. Environment-specific TOML file (e.g., `production.toml`)
//! 3. Base TOML file (`base.toml`)
//! 4. Default values
//!
//! ## Settings Directory
//!
//! `REINHARDT_CLOUD_CONFIG_DIR` overrides the settings directory for deployed
//! images; otherwise `settings/` next to this crate's manifest is used.
//!
//! ## Environment Selection
//!
//! The environment is determined by the `REINHARDT_ENV` environment variable:
//! - `local` or `development` → loads `local.toml`
//! - `staging` → loads `staging.toml`
//! - `production` → loads `production.toml`
//!
//! If `REINHARDT_ENV` is not set, it defaults to `local`.
//!
//! ## Environment Variable Interpolation
//!
//! `TomlFileSource` interpolates `${VAR}` syntax inside TOML string values
//! by default (since reinhardt-web v0.1.0-rc.27). The `${...}` syntax is
//! not valid in non-string TOML literals. Supported forms:
//!
//! - `${VAR}` — required; settings load fails if `VAR` is unset
//! - `${VAR:-default}` — falls back to `default` when `VAR` is unset
//! - `${VAR:?message}` — settings load fails with `message` when `VAR` is unset
//!
//! Interpolated strings are typed-coerced at deserialization time, so
//! `pool_size = "${DB_POOL_SIZE:-10}"` resolves directly to the field's
//! declared Rust type (e.g. `u16`) without manual parsing.

use reinhardt::conf::settings::PendingSettings;
use reinhardt::conf::settings::builder::{BuildError, SettingsBuilder};
use reinhardt::conf::settings::profile::Profile;
use reinhardt::conf::settings::scoped::ScopedSettings;
use reinhardt::conf::settings::sources::{DefaultSource, HighPriorityEnvSource, TomlFileSource};
use reinhardt::settings;
use std::env;

// Add fragments to extend settings: e.g. `#[settings(core: CoreSettings | cache: CacheSettings)]`
#[settings(core: CoreSettings | contacts: ContactSettings | migrations: MigrationSettings)]
pub struct ProjectSettings;

/// Get settings based on environment variable
///
/// Reads the REINHARDT_ENV environment variable to determine which settings to load.
/// Defaults to "local" if not set.
///
/// # Examples
///
/// ```no_run
/// use cloud_control_plane::config::settings::get_settings;
///
/// let settings = get_settings().expect("settings sources should load");
/// ```
///
/// # Errors
///
/// Returns an error when a settings source cannot be loaded or parsed.
pub fn get_settings() -> Result<PendingSettings<ProjectSettings>, BuildError> {
	settings_builder().build_pending_composed::<ProjectSettings>()
}

/// Merge settings without expanding unselected runtime secrets.
pub fn get_scoped_settings() -> Result<ScopedSettings, BuildError> {
	settings_builder().build_scoped()
}

fn settings_builder() -> SettingsBuilder {
	let profile_str = env::var("REINHARDT_ENV").unwrap_or_else(|_| "local".to_string());
	let profile = Profile::parse(&profile_str);

	// Resolve the managed project root independently of the caller's working directory.
	let base_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
	// Deployed images relocate `settings/`; the operator points the process at it
	// through `REINHARDT_CLOUD_CONFIG_DIR` (for example `/app/settings`).
	let settings_dir = env::var_os("REINHARDT_CLOUD_CONFIG_DIR")
		.map(std::path::PathBuf::from)
		.unwrap_or_else(|| base_dir.join("settings"));

	// Build settings by merging sources in priority order.
	// The composed and scoped paths use deep merging, so a
	// single key in `production.toml` overrides only that key — sibling
	// entries inside the same nested table inherit from `base.toml`.
	SettingsBuilder::new()
        .profile(profile)
        // Lowest priority: Default values
        .add_source(
            DefaultSource::new()
                .with_value("core", serde_json::json!({ "base_dir": base_dir }))
                .with_value("migrations", serde_json::json!({})),
        )
        // Medium priority: Base TOML file
        .add_source(TomlFileSource::new(settings_dir.join("base.toml")))
        // Profile priority: Environment-specific TOML file
        .add_source(TomlFileSource::new(
            settings_dir.join(format!("{}.toml", profile_str)),
        ))
        // Highest priority: explicit process environment overrides
        .add_source(HighPriorityEnvSource::new().with_prefix("REINHARDT_"))
}

/// Return plain project settings for consumers whose evaluator type is `ProjectSettings`.
pub fn get_shell_settings() -> ProjectSettings {
	get_settings()
		.expect("Failed to build settings")
		.resolve()
		.expect("Failed to resolve settings")
		.into_parts()
		.0
}

#[cfg(test)]
mod tests {
	use std::env;

	use rstest::rstest;
	use serial_test::serial;

	use crate::config::settings::get_settings;

	/// Process environment variables the committed settings require.
	const REQUIRED_ENV: [(&str, &str); 2] = [
		(
			"REINHARDT_CORE__SECRET_KEY",
			"test-only-secret-key-not-for-deployment",
		),
		("REINHARDT_DATABASE_PASSWORD", "test-only-database-password"),
	];

	/// Restores the previous value of every variable it overrides when dropped.
	struct EnvGuard {
		previous: Vec<(&'static str, Option<std::ffi::OsString>)>,
	}

	impl EnvGuard {
		fn set(vars: &[(&'static str, &str)]) -> Self {
			let previous = vars
				.iter()
				.map(|(name, value)| {
					let old = env::var_os(name);
					// SAFETY: callers are marked `#[serial(env_settings_load)]`, so no
					// other test reads or writes the process environment concurrently.
					unsafe { env::set_var(name, value) };
					(*name, old)
				})
				.collect();
			Self { previous }
		}
	}

	impl Drop for EnvGuard {
		fn drop(&mut self) {
			for (name, old) in self.previous.drain(..) {
				// SAFETY: see `EnvGuard::set`.
				unsafe {
					match old {
						Some(value) => env::set_var(name, value),
						None => env::remove_var(name),
					}
				}
			}
		}
	}

	#[rstest]
	#[serial(env_settings_load)]
	fn base_settings_load_secrets_from_the_environment() {
		// Arrange
		let mut vars = REQUIRED_ENV.to_vec();
		vars.push(("REINHARDT_ENV", "staging"));
		let _env = EnvGuard::set(&vars);

		// Act
		let settings = get_settings()
			.expect("settings sources should load")
			.resolve()
			.expect("settings should resolve");

		// Assert
		assert_eq!(
			settings.settings().core.secret_key,
			"test-only-secret-key-not-for-deployment"
		);
	}
}
