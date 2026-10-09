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
//! - `ci` → loads `ci.toml`
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
//! declared Rust type (e.g. `u16`) without manual parsing. A value that still
//! holds an unexpanded placeholder cannot be coerced into a number or boolean,
//! so such a setting fails to load instead of being used (SR-99).
//!
//! ## Startup validation
//!
//! [`get_settings`] and [`get_resolved_settings`] validate the merged settings
//! before returning them. The container server entry
//! (`crate::server::run`) loads its settings through [`get_resolved_settings`]
//! and hands them to `runserver`, and the runtime `manage` commands load them
//! through [`get_settings`], so both fail fast. Static `manage` commands that
//! use [`get_scoped_settings`] (for example `collectstatic`) resolve only the
//! settings they need and skip this validation by design. The checks are:
//!
//! - fragment validation for the active profile (`CoreSettings` requires a
//!   secret key, and a hardened production profile);
//! - required secrets (the application secret key, the database password, the
//!   Redis URL, and any configured token encryption key) must be non-empty and
//!   must not hold an unexpanded `${...}` or `$(...)` placeholder (SR-99);
//! - the `staging` and `production` profiles must be hardened: debug off,
//!   secure cookies, HTTPS redirect with HSTS, explicit allowed hosts, and no
//!   wildcard or localhost hosts and origins (SR-100).

use reinhardt::conf::settings::PendingSettings;
use reinhardt::conf::settings::builder::{BuildError, SettingsBuilder};
use reinhardt::conf::settings::composed::ComposedSettings;
use reinhardt::conf::settings::composed::ResolvedSettings;
use reinhardt::conf::settings::profile::Profile;
use reinhardt::conf::settings::scoped::ScopedSettings;
use reinhardt::conf::settings::secret_types::SecretString;
use reinhardt::conf::settings::sources::{DefaultSource, HighPriorityEnvSource, TomlFileSource};
use reinhardt::settings;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::{Path, PathBuf};

use crate::apps::accounts::server::settings::AccountsSettings;

/// Connection settings of the Redis instance backing sessions and short-lived
/// sign-in state.
///
/// The URL carries the Redis credentials (the operator composes it from
/// `REINHARDT_CLOUD_REDIS_PASSWORD`), so it is a secret and redacts itself in
/// `Debug` output.
#[settings(fragment = true, section = "redis")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RedisSettings {
	/// Redis URL, including credentials in deployed profiles. Required: a
	/// profile that omits `[redis]` fails at startup.
	#[setting(required)]
	pub url: SecretString,
}

// Add fragments to extend settings: e.g. `#[settings(core: CoreSettings | cache: CacheSettings)]`
#[settings(core: CoreSettings | contacts: ContactSettings | migrations: MigrationSettings | accounts: AccountsSettings | redis: RedisSettings)]
pub struct ProjectSettings;

/// Get settings based on environment variable
///
/// Reads the REINHARDT_ENV environment variable to determine which settings to load.
/// Defaults to "local" if not set.
///
/// Validation runs here, before the settings are handed out: a missing,
/// empty, or placeholder secret, or an unhardened `staging`/`production`
/// profile, stops the process with an error that names the offending setting
/// (never its value).
///
/// The caller still has to `resolve()` the returned value, because the
/// `manage` capability provider consumes the pending form; code that needs the
/// resolved settings itself should call [`get_resolved_settings`], which
/// resolves once.
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
/// Returns an error when a settings source cannot be loaded or parsed, or when
/// validation fails.
pub fn get_settings() -> Result<PendingSettings<ProjectSettings>, BuildError> {
	let pending = settings_builder().build_pending_composed::<ProjectSettings>()?;
	resolve_validated(&pending)?;
	Ok(pending)
}

/// Load, validate, and resolve the settings once.
///
/// Applies the same validation as [`get_settings`].
///
/// # Errors
///
/// Returns an error when a settings source cannot be loaded or parsed, or when
/// validation fails.
pub fn get_resolved_settings() -> Result<ResolvedSettings<ProjectSettings>, BuildError> {
	let pending = settings_builder().build_pending_composed::<ProjectSettings>()?;
	resolve_validated(&pending)
}

fn active_profile() -> Profile {
	Profile::parse(&env::var("REINHARDT_ENV").unwrap_or_else(|_| "local".to_string()))
}

fn resolve_validated(
	pending: &PendingSettings<ProjectSettings>,
) -> Result<ResolvedSettings<ProjectSettings>, BuildError> {
	let resolved = pending.resolve()?;
	let profile = active_profile();
	resolved
		.settings()
		.validate_fragments(&profile)
		.map_err(|error| BuildError::Validation(error.to_string()))?;
	validate_secrets(resolved.settings())?;
	if matches!(profile, Profile::Staging | Profile::Production) {
		let origins = pending
			.deserialize_section::<WebSocketOriginSettings>("ws_origin")
			.map(|section| section.policy.origins)?;
		validate_hardened_profile(resolved.settings(), &origins)?;
	}
	Ok(resolved)
}

/// The part of the `[ws_origin]` table the hardening check inspects.
#[derive(Deserialize)]
struct WebSocketOriginSettings {
	policy: WebSocketOriginPolicy,
}

#[derive(Deserialize)]
struct WebSocketOriginPolicy {
	#[serde(default)]
	origins: Vec<String>,
}

/// The smallest `core.secret_key` accepted by the hardened profiles.
const MIN_DEPLOYED_SECRET_KEY_LEN: usize = 32;

/// Reject a secret that is empty or still holds a placeholder (SR-99).
///
/// The error names the setting, never the value.
fn reject_unusable_secret(setting: &str, value: &str) -> Result<(), BuildError> {
	if value.trim().is_empty() {
		return Err(BuildError::Validation(format!(
			"required secret `{setting}` is empty"
		)));
	}
	if value.contains("${") || value.contains("$(") {
		return Err(BuildError::Validation(format!(
			"required secret `{setting}` still holds an unexpanded placeholder"
		)));
	}
	Ok(())
}

fn validate_secrets(settings: &ProjectSettings) -> Result<(), BuildError> {
	reject_unusable_secret("core.secret_key", &settings.core.secret_key)?;
	for (name, database) in &settings.core.databases {
		let password = database
			.password
			.as_ref()
			.map_or("", SecretString::expose_secret);
		reject_unusable_secret(&format!("core.databases.{name}.password"), password)?;
	}
	reject_unusable_secret("redis.url", settings.redis.url.expose_secret())?;
	for (setting, secret) in [
		(
			"accounts.token_encryption_key",
			settings.accounts.token_encryption_key.as_ref(),
		),
		(
			"accounts.token_encryption_retired_keys",
			settings.accounts.token_encryption_retired_keys.as_ref(),
		),
	] {
		if let Some(secret) = secret.filter(|secret| !secret.is_empty()) {
			reject_unusable_secret(setting, secret.expose_secret())?;
		}
	}
	// Sign-in must not start without a usable provider-token key (SR-06).
	settings
		.accounts
		.token_keyring(&settings.core.secret_key)
		.map_err(|error| BuildError::Validation(format!("accounts.token_encryption_*: {error}")))?;
	Ok(())
}

fn is_local_host(host: &str) -> bool {
	let host = host
		.trim_start_matches("http://")
		.trim_start_matches("https://");
	let host = host.split(['/', ':']).next().unwrap_or(host);
	matches!(
		host,
		"localhost" | "127.0.0.1" | "::1" | "[::1]" | "0.0.0.0"
	)
}

/// Enforce the production defaults on the `staging` and `production` profiles
/// (SR-100).
fn validate_hardened_profile(
	settings: &ProjectSettings,
	websocket_origins: &[String],
) -> Result<(), BuildError> {
	let core = &settings.core;
	let security = &core.security;
	let mut problems = Vec::new();
	if core.debug {
		problems.push("core.debug must be false");
	}
	if core.secret_key.len() < MIN_DEPLOYED_SECRET_KEY_LEN {
		problems.push("core.secret_key must be at least 32 bytes");
	}
	if !security.session_cookie_secure {
		problems.push("core.security.session_cookie_secure must be true");
	}
	if !security.csrf_cookie_secure {
		problems.push("core.security.csrf_cookie_secure must be true");
	}
	if !security.secure_ssl_redirect {
		problems.push("core.security.secure_ssl_redirect must be true");
	}
	if security.secure_hsts_seconds.unwrap_or(0) == 0 {
		problems.push("core.security.secure_hsts_seconds must be greater than zero");
	}
	if core.allowed_hosts.is_empty() {
		problems.push("core.allowed_hosts must list hosts explicitly");
	}
	if core
		.allowed_hosts
		.iter()
		.any(|host| host.contains('*') || host.starts_with('.') || is_local_host(host))
	{
		problems.push("core.allowed_hosts must not contain wildcard or localhost entries");
	}
	if websocket_origins.iter().any(|origin| {
		origin.contains('*') || is_local_host(origin) || !origin.starts_with("https://")
	}) {
		problems.push(
			"ws_origin.policy.origins must be https origins without wildcard or localhost entries",
		);
	}
	if problems.is_empty() {
		Ok(())
	} else {
		Err(BuildError::Validation(format!(
			"profile is not hardened: {}",
			problems.join("; ")
		)))
	}
}

/// Merge settings without expanding unselected runtime secrets.
pub fn get_scoped_settings() -> Result<ScopedSettings, BuildError> {
	settings_builder().build_scoped()
}

/// Resolve the settings directory: `REINHARDT_CLOUD_CONFIG_DIR` when set,
/// otherwise `settings/` under `base_dir`.
fn resolve_settings_dir(base_dir: &Path) -> PathBuf {
	env::var_os("REINHARDT_CLOUD_CONFIG_DIR")
		.map(PathBuf::from)
		.unwrap_or_else(|| base_dir.join("settings"))
}

fn settings_builder() -> SettingsBuilder {
	let profile_str = env::var("REINHARDT_ENV").unwrap_or_else(|_| "local".to_string());
	let profile = Profile::parse(&profile_str);

	// Resolve the managed project root independently of the caller's working directory.
	let base_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
	// Deployed images relocate `settings/`; the operator points the process at it
	// through `REINHARDT_CLOUD_CONFIG_DIR` (for example `/app/settings`).
	let settings_dir = resolve_settings_dir(&base_dir);

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
                .with_value("migrations", serde_json::json!({}))
                .with_value("accounts", serde_json::json!({})),
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
	get_resolved_settings()
		.expect("Failed to build settings")
		.into_parts()
		.0
}

#[cfg(test)]
pub(crate) mod test_support;
#[cfg(test)]
mod tests;
