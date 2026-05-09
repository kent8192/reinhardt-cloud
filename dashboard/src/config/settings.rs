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
use reinhardt::conf::settings::sources::{HighPriorityEnvSource, TomlFileSource};
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

/// Build merged settings from all configuration sources, returning errors
/// for missing required env vars (so callers — including tests — can
/// distinguish a startup misconfiguration from an unrelated panic).
///
/// Sources are merged in priority order (lowest to highest):
/// 1. Default values (CoreSettings provides its own defaults via serde)
/// 2. Base TOML file (`base.toml`)
/// 3. Environment-specific TOML file (e.g., `local.toml`)
/// 4. Environment variables with `REINHARDT_` prefix (highest)
fn try_build_settings() -> Result<ProjectSettings, Box<dyn std::error::Error + Send + Sync>> {
	let profile_str = profile_name();
	let settings_dir = resolve_settings_dir();

	SettingsBuilder::new()
		.profile(Profile::parse(&profile_str))
		// `with_interpolation()` expands `${VAR}` / `${VAR:-default}` in TOML
		// string values against process env at load time, so a single TOML
		// file can host environment-specific knobs without a dedicated
		// profile per environment. Combined with reinhardt-conf's typed
		// coercion (kent8192/reinhardt-web#4232), interpolated strings also
		// deserialize into typed fields (`port: u16`, `debug: bool`, ...).
		.add_source(TomlFileSource::new(settings_dir.join("base.toml")).with_interpolation())
		.add_source(
			TomlFileSource::new(settings_dir.join(format!("{}.toml", profile_str)))
				.with_interpolation(),
		)
		// Highest priority: Environment variables (for container/CI overrides)
		.add_source(HighPriorityEnvSource::new().with_prefix("REINHARDT_"))
		.build_composed()
		.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
}

/// Build merged settings or panic.
///
/// Production callers (`get_settings`) use this; tests that need to inspect
/// the error path call [`try_build_settings`] directly.
fn build_settings() -> ProjectSettings {
	try_build_settings().expect("Failed to build settings")
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
	get_top_level_string("redis_url", "REINHARDT_CLOUD_REDIS_URL")
}

/// Get the JWT signing secret from settings or environment.
///
/// Priority (highest to lowest):
/// 1. `jwt_secret` top-level key in the active TOML settings file
/// 2. `REINHARDT_CLOUD_JWT_SECRET` environment variable (fallback for CI/container overrides)
///
/// Returns `None` if the secret is not configured in either source.
///
/// Issue: kent8192/reinhardt-cloud#494
pub fn get_jwt_secret() -> Option<String> {
	get_top_level_string("jwt_secret", "REINHARDT_CLOUD_JWT_SECRET")
}

/// Resolve a top-level TOML string key with an environment-variable fallback.
///
/// Reads `base.toml` + `<profile>.toml` and returns the merged top-level
/// `key` value as a `String`. If absent, reads `env_var` from the process
/// environment. Returns `None` if neither source supplies the value.
fn get_top_level_string(key: &str, env_var: &str) -> Option<String> {
	let profile_str = profile_name();
	let settings_dir = resolve_settings_dir();
	let from_toml = SettingsBuilder::new()
		.add_source(TomlFileSource::new(settings_dir.join("base.toml")).with_interpolation())
		.add_source(
			TomlFileSource::new(settings_dir.join(format!("{}.toml", profile_str)))
				.with_interpolation(),
		)
		.build()
		.ok()
		.and_then(|merged| {
			merged
				.get_raw(key)
				.and_then(|v| v.as_str())
				.map(String::from)
		});

	if from_toml.is_some() {
		return from_toml;
	}

	env::var(env_var).ok()
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	use serial_test::serial;

	/// Required env vars for the production profile after #588 made
	/// `production.toml` self-contained via `${VAR:?msg}` interpolation.
	const PRODUCTION_REQUIRED_ENV_VARS: &[&str] = &[
		"REINHARDT_CLOUD_JWT_SECRET",
		"REINHARDT_CLOUD_REDIS_URL",
		"REINHARDT_CORE__SECRET_KEY",
		"REINHARDT_DATABASE_PASSWORD",
		"REINHARDT_EMAIL__HOST",
	];

	/// Snapshot env vars before mutating so the test can restore them on the
	/// way out. Returns `(name, original_value)` pairs.
	fn snapshot_env(names: &[&str]) -> Vec<(String, Option<String>)> {
		names
			.iter()
			.map(|n| ((*n).to_string(), env::var(n).ok()))
			.collect()
	}

	fn restore_env(saved: &[(String, Option<String>)]) {
		// SAFETY: `set_var`/`remove_var` are racy across threads. The
		// `#[serial(env_settings_load)]` attribute on the calling tests
		// guarantees only one of these runs at a time within this group.
		unsafe {
			for (name, value) in saved {
				match value {
					Some(v) => env::set_var(name, v),
					None => env::remove_var(name),
				}
			}
		}
	}

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

	#[rstest]
	fn test_get_jwt_secret_reads_from_local_toml() {
		// Arrange / Act — local.toml ships with `jwt_secret = "test-secret-..."`,
		// which `get_jwt_secret` MUST surface even when no env var is set.
		// Issue: #494
		let secret = get_jwt_secret();

		// Assert
		assert!(secret.is_some(), "expected jwt_secret from local.toml");
		assert!(!secret.unwrap().is_empty());
	}

	/// `production.toml` rewritten in #588 must fail-fast at startup if any
	/// required `${VAR:?msg}` env var is missing — otherwise the dashboard
	/// would silently boot with a placeholder secret. The error message must
	/// name the missing env var so an operator can fix the deployment without
	/// guessing.
	#[rstest]
	#[serial(env_settings_load)]
	fn test_production_profile_fails_fast_when_required_env_vars_missing() {
		// Arrange — snapshot every env var this test mutates so other tests in
		// the same group see the original process environment afterwards.
		let mut watched: Vec<&str> = vec!["REINHARDT_ENV"];
		watched.extend_from_slice(PRODUCTION_REQUIRED_ENV_VARS);
		let saved = snapshot_env(&watched);

		// SAFETY: see `restore_env`. `#[serial(env_settings_load)]` provides
		// the cross-test exclusion that `set_var`/`remove_var` need.
		unsafe {
			env::set_var("REINHARDT_ENV", "production");
			for name in PRODUCTION_REQUIRED_ENV_VARS {
				env::remove_var(name);
			}
		}

		// Act
		let outcome = try_build_settings();

		// Assert — error must mention at least one of the required env vars.
		// Restore env BEFORE asserting so a failed assertion cannot leak state.
		restore_env(&saved);

		let err = outcome.expect_err(
			"production.toml must reject startup with no env vars set — see \
			 issue #588 for the fail-fast contract",
		);
		let msg = err.to_string();
		let mut full = msg.clone();
		let mut source = err.source();
		while let Some(s) = source {
			full.push_str("\nCaused by: ");
			full.push_str(&s.to_string());
			source = s.source();
		}

		assert!(
			PRODUCTION_REQUIRED_ENV_VARS
				.iter()
				.any(|v| full.contains(v)),
			"error message should name at least one required env var, got:\n{full}",
		);
	}

	/// With every required env var set, `production.toml` round-trips into a
	/// `ProjectSettings` whose typed fields (`port: u16`, `use_tls: bool`,
	/// `allowed_hosts: Vec<String>`) deserialize correctly and the
	/// `${VAR:-default}` form picks up overrides.
	#[rstest]
	#[serial(env_settings_load)]
	fn test_production_profile_round_trips_with_required_env_vars() {
		// Arrange
		let watched: Vec<&str> = vec![
			"REINHARDT_ENV",
			"REINHARDT_CLOUD_JWT_SECRET",
			"REINHARDT_CLOUD_REDIS_URL",
			"REINHARDT_CORE__SECRET_KEY",
			"REINHARDT_DATABASE_PASSWORD",
			"REINHARDT_DATABASE_HOST",
			"REINHARDT_EMAIL__HOST",
		];
		let saved = snapshot_env(&watched);

		// SAFETY: see `restore_env`.
		unsafe {
			env::set_var("REINHARDT_ENV", "production");
			env::set_var(
				"REINHARDT_CLOUD_JWT_SECRET",
				"test-jwt-secret-32-bytes-of-random-data!!",
			);
			env::set_var("REINHARDT_CLOUD_REDIS_URL", "redis://test-redis:6379/0");
			env::set_var("REINHARDT_CORE__SECRET_KEY", "test-core-secret-key");
			env::set_var("REINHARDT_DATABASE_PASSWORD", "test-db-password");
			// Override the default (db.production.internal) to prove
			// `${VAR:-default}` honours env overrides.
			env::set_var("REINHARDT_DATABASE_HOST", "override.example.invalid");
			env::set_var("REINHARDT_EMAIL__HOST", "smtp.test-provider.invalid");
		}

		// Act
		let outcome = try_build_settings();

		// Assert — restore env first, then inspect.
		restore_env(&saved);

		let settings = outcome.expect("production.toml should load with env vars set");
		// Typed numeric / bool fields survive the TOML→struct hop.
		let db = &settings.core.databases["default"];
		assert_eq!(db.port, Some(5432), "port must deserialize as u16");
		assert_eq!(
			db.host.as_deref(),
			Some("override.example.invalid"),
			"${{VAR:-default}} must honour env override",
		);
		assert!(
			!settings.core.allowed_hosts.is_empty(),
			"allowed_hosts must deserialize as Vec<String>",
		);
		// Production profile flips security knobs on.
		assert!(!settings.core.debug);
		assert!(settings.core.security.secure_ssl_redirect);
		assert!(settings.core.security.session_cookie_secure);
	}
}
