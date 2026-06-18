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
//! 1. Environment variables with `REINHARDT_` prefix
//! 2. Environment-specific TOML file (e.g., `production.toml`)
//! 3. Base TOML file (`base.toml`)
//! 4. Default values
//!
//! `.env.<profile>` and `.env` files in the dashboard crate root are loaded
//! before TOML files so `${VAR}` interpolation can consume local dotenv values.
//! Existing process environment variables are never overwritten by dotenv files.
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

use reinhardt::conf::settings::profile::Profile;
use reinhardt::conf::settings::sources::{
	ConfigSource, DefaultSource, DotEnvSource, EnvSource, SourceError, TomlFileSource,
};
use reinhardt::conf::{
	ContactSettings, EmailSettings, I18nSettings, MediaSettings, StaticSettings,
	settings::builder::{BuildError, SettingsBuilder},
};
use reinhardt::di::{Depends, FactoryOutput};
use reinhardt::settings;
use std::env;
use std::path::{Path, PathBuf};

/// Composable project settings using the `#[settings]` macro.
///
/// Each fragment maps to a TOML section:
/// - `[core]` → `CoreSettings` (security fields nested under `[core.security]`)
/// - `[i18n]` → `I18nSettings`
/// - `[static_files]` → `StaticSettings`
/// - `[media]` → `MediaSettings`
/// - `[cors]` → `CorsSettings` (includes `allow_origins` used by `OriginGuardMiddleware`)
/// - `[email]` → `EmailSettings` (SMTP backend configuration for transactional emails)
/// - `[contacts]` → `ContactSettings` (required by settings-aware management commands)
#[settings(core: CoreSettings | I18nSettings | static_files: StaticSettings | MediaSettings | CorsSettings | EmailSettings | contacts: ContactSettings)]
pub struct ProjectSettings;

#[reinhardt::di::injectable_key]
pub struct ProjectSettingsKey;

pub type ProjectSettingsDepends = Depends<ProjectSettingsKey, ProjectSettings>;

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

fn resolve_dotenv_dir(settings_dir: &Path) -> PathBuf {
	settings_dir
		.parent()
		.map(Path::to_path_buf)
		.unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

/// Get the active environment profile name.
///
/// Reads `REINHARDT_ENV` and defaults to `"local"`.
fn profile_name() -> String {
	env::var("REINHARDT_ENV").unwrap_or_else(|_| "local".to_string())
}

fn default_project_settings_source() -> DefaultSource {
	DefaultSource::new()
		.with_value(
			"i18n",
			serde_json::to_value(I18nSettings::default())
				.expect("I18nSettings default should serialize"),
		)
		.with_value(
			"static_files",
			serde_json::to_value(StaticSettings::default())
				.expect("StaticSettings default should serialize"),
		)
		.with_value(
			"media",
			serde_json::to_value(MediaSettings::default())
				.expect("MediaSettings default should serialize"),
		)
		.with_value(
			"email",
			serde_json::to_value(EmailSettings::default())
				.expect("EmailSettings default should serialize"),
		)
		.with_value(
			"contacts",
			serde_json::to_value(ContactSettings::default())
				.expect("ContactSettings default should serialize"),
		)
}

struct DashboardDotEnvSource {
	path: PathBuf,
}

impl DashboardDotEnvSource {
	fn new(path: impl Into<PathBuf>) -> Self {
		Self { path: path.into() }
	}
}

impl ConfigSource for DashboardDotEnvSource {
	fn load(&self) -> Result<indexmap::IndexMap<String, serde_json::Value>, SourceError> {
		DotEnvSource::new().with_path(&self.path).load()
	}

	fn priority(&self) -> u8 {
		20
	}

	fn description(&self) -> String {
		format!("Dashboard dotenv file: {}", self.path.display())
	}
}

fn add_dashboard_dotenv_sources(
	builder: SettingsBuilder,
	dotenv_dir: &Path,
	profile_str: &str,
) -> SettingsBuilder {
	let profile_path = dotenv_dir.join(format!(".env.{profile_str}"));
	builder
		.add_source(DashboardDotEnvSource::new(profile_path))
		.add_source(DashboardDotEnvSource::new(dotenv_dir.join(".env")))
}

fn load_dashboard_dotenv_files(settings_dir: &Path, profile_str: &str) -> Result<(), SourceError> {
	let dotenv_dir = resolve_dotenv_dir(settings_dir);
	DashboardDotEnvSource::new(dotenv_dir.join(format!(".env.{profile_str}"))).load()?;
	DashboardDotEnvSource::new(dotenv_dir.join(".env")).load()?;
	Ok(())
}

/// Build merged settings from all configuration sources, returning errors
/// for missing required env vars (so callers — including tests — can
/// distinguish a startup misconfiguration from an unrelated panic).
///
/// Sources are merged in priority order (lowest to highest):
/// 1. Default values (CoreSettings provides its own defaults via serde)
/// 2. Dashboard dotenv files (`.env`, `.env.<profile>`)
/// 3. Base TOML file (`base.toml`)
/// 4. Environment-specific TOML file (e.g., `local.toml`)
/// 5. Environment variables with `REINHARDT_` prefix (highest)
fn try_build_settings() -> Result<ProjectSettings, BuildError> {
	let profile_str = profile_name();
	let settings_dir = resolve_settings_dir();
	let dotenv_dir = resolve_dotenv_dir(&settings_dir);

	let builder = add_dashboard_dotenv_sources(SettingsBuilder::new(), &dotenv_dir, &profile_str);

	let mut settings: ProjectSettings = builder
		.profile(Profile::parse(&profile_str))
		.add_source(default_project_settings_source())
		.add_source(TomlFileSource::new(settings_dir.join("base.toml")))
		.add_source(TomlFileSource::new(
			settings_dir.join(format!("{}.toml", profile_str)),
		))
		// Highest priority: process environment values for container/CI overrides.
		.add_source(EnvSource::new().with_prefix("REINHARDT_"))
		.build_composed()?;
	apply_email_env_overrides(&mut settings)?;
	Ok(settings)
}

fn parse_email_env<T>(name: &str) -> Result<Option<T>, BuildError>
where
	T: std::str::FromStr,
	T::Err: std::fmt::Display,
{
	match env::var(name) {
		Ok(value) => value
			.parse::<T>()
			.map(Some)
			.map_err(|e| BuildError::Deserialization(format!("Invalid {name}: {e}"))),
		Err(env::VarError::NotPresent) => Ok(None),
		Err(e) => Err(BuildError::Deserialization(format!("Invalid {name}: {e}"))),
	}
}

fn apply_email_env_overrides(settings: &mut ProjectSettings) -> Result<(), BuildError> {
	if let Ok(v) = env::var("REINHARDT_EMAIL__BACKEND") {
		settings.email.backend = v;
	}
	if let Ok(v) = env::var("REINHARDT_EMAIL__HOST") {
		settings.email.host = v;
	}
	if let Some(v) = parse_email_env("REINHARDT_EMAIL__PORT")? {
		settings.email.port = v;
	}
	if let Ok(v) = env::var("REINHARDT_EMAIL__USERNAME") {
		settings.email.username = Some(v);
	}
	if let Ok(v) = env::var("REINHARDT_EMAIL__PASSWORD") {
		settings.email.password = Some(v);
	}
	if let Some(v) = parse_email_env("REINHARDT_EMAIL__USE_TLS")? {
		settings.email.use_tls = v;
	}
	if let Some(v) = parse_email_env("REINHARDT_EMAIL__USE_SSL")? {
		settings.email.use_ssl = v;
	}
	if let Ok(v) = env::var("REINHARDT_EMAIL__FROM_EMAIL") {
		settings.email.from_email = v;
	}
	if let Some(v) = parse_email_env("REINHARDT_EMAIL__TIMEOUT")? {
		settings.email.timeout = Some(v);
	}
	Ok(())
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

/// DI factory for the project settings snapshot.
///
/// Factories that need a settings fragment should inject this composed
/// `ProjectSettings` value rather than rebuilding settings independently.
#[cfg(native)]
#[reinhardt::di::injectable(scope = "singleton")]
async fn create_project_settings() -> FactoryOutput<ProjectSettingsKey, ProjectSettings> {
	FactoryOutput::new(build_settings())
}

/// Get Redis URL from settings or environment.
///
/// Priority (highest to lowest):
/// 1. `REINHARDT_CLOUD_REDIS_URL` environment variable (container/operator override)
/// 2. `redis_url` key in the active TOML settings file
///
/// Returns `None` if the Redis URL is not configured in either source.
pub fn get_redis_url() -> Option<String> {
	get_env_or_top_level_string("REINHARDT_CLOUD_REDIS_URL", "redis_url")
}

/// Log backend selected for the dashboard's gRPC `LogServiceServer`.
///
/// `Memory` (default) uses an in-process ring buffer (`LocalLogService`); `Loki`
/// routes reads to a Loki instance via the read-oriented `LokiLogService`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogBackend {
	/// In-memory ring buffer (dev/test).
	#[default]
	Memory,
	/// Loki read path (production).
	Loki,
}

/// Resolve the configured log backend.
///
/// Priority (highest to lowest):
/// 1. `REINHARDT_CLOUD_LOG_BACKEND` environment variable
/// 2. `log_backend` key in the active TOML settings file
/// 3. `LogBackend::Memory` default
pub fn get_log_backend() -> LogBackend {
	match get_env_or_top_level_string("REINHARDT_CLOUD_LOG_BACKEND", "log_backend")
		.as_deref()
		.map(str::to_ascii_lowercase)
		.as_deref()
	{
		Some("loki") => LogBackend::Loki,
		Some("memory") => LogBackend::Memory,
		_ => LogBackend::Memory,
	}
}

/// Resolve the Loki base URL used when the backend is [`LogBackend::Loki`].
///
/// Priority (highest to lowest):
/// 1. `REINHARDT_CLOUD_LOKI_ENDPOINT` environment variable
/// 2. `loki_endpoint` key in the active TOML settings file
/// 3. The in-cluster default `http://loki.monitoring.svc.cluster.local:3100`
pub fn get_loki_endpoint() -> String {
	get_env_or_top_level_string("REINHARDT_CLOUD_LOKI_ENDPOINT", "loki_endpoint")
		.unwrap_or_else(|| "http://loki.monitoring.svc.cluster.local:3100".to_string())
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
	let dotenv_dir = resolve_dotenv_dir(&settings_dir);
	let builder = add_dashboard_dotenv_sources(SettingsBuilder::new(), &dotenv_dir, &profile_str);

	let from_toml = builder
		.add_source(TomlFileSource::new(settings_dir.join("base.toml")))
		.add_source(TomlFileSource::new(
			settings_dir.join(format!("{}.toml", profile_str)),
		))
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

/// Resolve an environment variable with a top-level TOML string fallback.
///
/// Containerized deployments inject runtime infrastructure endpoints after
/// TOML settings are selected, so these env vars must be able to override
/// profile defaults such as `ci.toml`.
fn get_env_or_top_level_string(env_var: &str, key: &str) -> Option<String> {
	let profile_str = profile_name();
	let settings_dir = resolve_settings_dir();
	let _ = load_dashboard_dotenv_files(&settings_dir, &profile_str);

	env::var(env_var)
		.ok()
		.or_else(|| get_top_level_string(key, env_var))
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	use serial_test::serial;
	use std::ffi::OsString;
	use std::fs;
	use std::time::{SystemTime, UNIX_EPOCH};

	/// Required env vars for deployed profiles after #588 made the profile
	/// TOML files self-contained via `${VAR:?msg}` interpolation.
	const DEPLOYED_REQUIRED_ENV_VARS: &[&str] = &[
		"REINHARDT_CLOUD_JWT_SECRET",
		"REINHARDT_CLOUD_REDIS_URL",
		"REINHARDT_CORE__SECRET_KEY",
		"REINHARDT_DATABASE_PASSWORD",
		"REINHARDT_EMAIL__HOST",
	];

	const PROFILE_DEFAULT_OVERRIDE_ENV_VARS: &[&str] = &[
		"REINHARDT_I18N__LANGUAGE_CODE",
		"REINHARDT_I18N__TIME_ZONE",
		"REINHARDT_STATIC_FILES__URL",
		"REINHARDT_STATIC_FILES__ROOT",
		"REINHARDT_MEDIA__URL",
		"REINHARDT_MEDIA__ROOT",
	];

	/// RAII guard that snapshots a set of env vars on construction and restores
	/// them on `Drop`, so a panicking test cannot leak mutated env into the
	/// next test in the `env_settings_load` serial group. Uses `OsString` so
	/// non-UTF-8 values round-trip losslessly instead of being silently
	/// dropped by `env::var(..).ok()`.
	struct EnvSnapshot {
		saved: Vec<(&'static str, Option<OsString>)>,
	}

	impl EnvSnapshot {
		fn new(names: &[&'static str]) -> Self {
			Self {
				saved: names.iter().map(|n| (*n, env::var_os(n))).collect(),
			}
		}
	}

	impl Drop for EnvSnapshot {
		fn drop(&mut self) {
			// SAFETY: `set_var`/`remove_var` are racy across threads. Every
			// caller holds `#[serial(env_settings_load)]`, which guarantees
			// only one test in this group mutates env at a time.
			unsafe {
				for (name, value) in &self.saved {
					match value {
						Some(v) => env::set_var(name, v),
						None => env::remove_var(name),
					}
				}
			}
		}
	}

	struct TempSettingsDir {
		path: PathBuf,
	}

	impl TempSettingsDir {
		fn with_profile(profile: &str) -> Self {
			let unique = SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.expect("system time should be after UNIX_EPOCH")
				.as_nanos();
			let path = env::temp_dir().join(format!("reinhardt-cloud-{profile}-settings-{unique}"));
			fs::create_dir_all(&path).expect("temporary settings directory should be created");
			fs::copy(
				Path::new(env!("CARGO_MANIFEST_DIR"))
					.join("settings")
					.join(format!("{profile}.toml")),
				path.join(format!("{profile}.toml")),
			)
			.expect("profile settings file should be copied");
			Self { path }
		}

		fn path(&self) -> &Path {
			&self.path
		}

		fn profile_path(&self, profile: &str) -> PathBuf {
			self.path.join(format!("{profile}.toml"))
		}
	}

	impl Drop for TempSettingsDir {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.path);
		}
	}

	fn set_deployed_profile_env(profile: &str, config_dir: &Path) -> EnvSnapshot {
		let mut watched: Vec<&'static str> = vec!["REINHARDT_CLOUD_CONFIG_DIR", "REINHARDT_ENV"];
		watched.extend_from_slice(DEPLOYED_REQUIRED_ENV_VARS);
		watched.extend_from_slice(PROFILE_DEFAULT_OVERRIDE_ENV_VARS);
		let guard = EnvSnapshot::new(&watched);

		// SAFETY: callers use `#[serial(env_settings_load)]`, which provides
		// the cross-test exclusion that `set_var`/`remove_var` need.
		unsafe {
			env::set_var("REINHARDT_CLOUD_CONFIG_DIR", config_dir);
			env::set_var("REINHARDT_ENV", profile);
			env::set_var(
				"REINHARDT_CLOUD_JWT_SECRET",
				"test-jwt-secret-32-bytes-of-random-data!!",
			);
			env::set_var("REINHARDT_CLOUD_REDIS_URL", "redis://test-redis:6379/0");
			env::set_var("REINHARDT_CORE__SECRET_KEY", "test-core-secret-key");
			env::set_var("REINHARDT_DATABASE_PASSWORD", "test-db-password");
			env::set_var("REINHARDT_EMAIL__HOST", "smtp.test-provider.invalid");
			for name in PROFILE_DEFAULT_OVERRIDE_ENV_VARS {
				env::remove_var(name);
			}
		}

		guard
	}

	fn assert_profile_file_contains_runtime_sections(profile_path: &Path) {
		let values = TomlFileSource::new(profile_path)
			.load()
			.expect("profile settings file should parse");

		for section in ["i18n", "static_files", "media"] {
			assert!(
				values.contains_key(section),
				"profile TOML must define [{section}] directly",
			);
		}
	}

	fn write_minimal_local_settings(config_dir: &Path, secret_key_expr: &str) {
		fs::create_dir_all(config_dir).expect("temporary settings directory should be created");
		fs::write(
			config_dir.join("local.toml"),
			format!(
				r#"
[core]
debug = true
secret_key = "{secret_key_expr}"
allowed_hosts = ["localhost"]
root_urlconf = ""
middleware = []

[core.security]
append_slash = true
session_cookie_secure = false
csrf_cookie_secure = false
secure_ssl_redirect = false
secure_hsts_include_subdomains = false
secure_hsts_preload = false

[core.databases.default]
engine = "postgresql"
host = "localhost"
port = 5432
name = "test"
user = "postgres"
password = {{ secret = "test-password" }}
options = {{}}

[cors]
allow_origins = ["http://localhost:8000"]
"#
			),
		)
		.expect("local settings file should be written");
	}

	#[rstest]
	#[serial(env_settings_load)]
	fn test_get_settings() {
		// Arrange / Act
		let settings = get_settings();

		// Assert
		assert!(!settings.core.secret_key.is_empty());
	}

	#[rstest]
	#[serial(env_settings_load)]
	fn test_core_settings_fields() {
		// Arrange / Act
		let settings = get_settings();

		// Assert
		assert!(settings.core.debug);
		assert!(!settings.core.allowed_hosts.is_empty());
		assert!(settings.core.databases.contains_key("default"));
	}

	#[rstest]
	#[serial(env_settings_load)]
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
	#[serial(env_settings_load)]
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
	#[serial(env_settings_load)]
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
	#[serial(env_settings_load)]
	fn test_static_files_settings() {
		// Arrange / Act
		let settings = get_settings();

		// Assert
		assert_eq!(settings.static_files.url, "/static/");
		assert!(!settings.static_files.root.as_os_str().is_empty());
	}

	#[rstest]
	#[serial(env_settings_load)]
	fn test_media_settings() {
		// Arrange / Act
		let settings = get_settings();

		// Assert
		assert_eq!(settings.media.url, "/media/");
		assert!(!settings.media.root.as_os_str().is_empty());
	}

	#[rstest]
	#[serial(env_settings_load)]
	fn test_fragment_defaults_load_when_base_toml_is_absent() {
		// Arrange
		let watched: Vec<&'static str> = vec!["REINHARDT_CLOUD_CONFIG_DIR", "REINHARDT_ENV"];
		let _guard = EnvSnapshot::new(&watched);
		let unique = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("system time should be after UNIX_EPOCH")
			.as_nanos();
		let config_dir =
			env::temp_dir().join(format!("reinhardt-cloud-settings-defaults-{unique}"));
		fs::create_dir_all(&config_dir).expect("temporary settings directory should be created");
		fs::write(
			config_dir.join("local.toml"),
			r#"
[core]
debug = true
secret_key = "test-secret"
allowed_hosts = ["localhost"]
root_urlconf = ""
middleware = []

[core.security]
append_slash = true
session_cookie_secure = false
csrf_cookie_secure = false
secure_ssl_redirect = false
secure_hsts_include_subdomains = false
secure_hsts_preload = false

[core.databases.default]
engine = "postgresql"
host = "localhost"
port = 5432
name = "test"
user = "postgres"
password = { secret = "test-password" }
options = {}

[cors]
allow_origins = ["http://localhost:8000"]
"#,
		)
		.expect("local settings file should be written");

		// SAFETY: `#[serial(env_settings_load)]` provides the cross-test
		// exclusion that `set_var` needs.
		unsafe {
			env::set_var("REINHARDT_CLOUD_CONFIG_DIR", &config_dir);
			env::set_var("REINHARDT_ENV", "local");
		}

		// Act
		let outcome = try_build_settings();
		let _ = fs::remove_dir_all(&config_dir);

		// Assert
		let settings =
			outcome.expect("fragment defaults should fill sections absent from local.toml");
		assert_eq!(settings.i18n.language_code, "en-us");
		assert_eq!(settings.static_files.url, "/static/");
		assert_eq!(settings.media.url, "/media/");
		assert_eq!(settings.email.port, 25);
	}

	#[rstest]
	#[serial(env_settings_load)]
	fn dotenv_profile_file_feeds_toml_interpolation() {
		// Arrange
		let watched: Vec<&'static str> = vec![
			"REINHARDT_CLOUD_CONFIG_DIR",
			"REINHARDT_ENV",
			"REINHARDT_CORE__SECRET_KEY",
		];
		let _guard = EnvSnapshot::new(&watched);
		let unique = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("system time should be after UNIX_EPOCH")
			.as_nanos();
		let root = env::temp_dir().join(format!("reinhardt-cloud-dotenv-local-{unique}"));
		let config_dir = root.join("settings");
		write_minimal_local_settings(
			&config_dir,
			"${REINHARDT_CORE__SECRET_KEY:?missing test secret}",
		);
		fs::write(
			root.join(".env.local"),
			"REINHARDT_CORE__SECRET_KEY=dotenv-profile-secret\n",
		)
		.expect("profile dotenv file should be written");

		// SAFETY: `#[serial(env_settings_load)]` provides the cross-test
		// exclusion that `set_var`/`remove_var` need.
		unsafe {
			env::set_var("REINHARDT_CLOUD_CONFIG_DIR", &config_dir);
			env::set_var("REINHARDT_ENV", "local");
			env::remove_var("REINHARDT_CORE__SECRET_KEY");
		}

		// Act
		let outcome = try_build_settings();
		let _ = fs::remove_dir_all(&root);

		// Assert
		let settings = outcome.expect(".env.local should feed TOML interpolation");
		assert_eq!(settings.core.secret_key, "dotenv-profile-secret");
	}

	#[rstest]
	#[serial(env_settings_load)]
	fn process_env_overrides_dotenv_profile_file() {
		// Arrange
		let watched: Vec<&'static str> = vec![
			"REINHARDT_CLOUD_CONFIG_DIR",
			"REINHARDT_ENV",
			"REINHARDT_CORE__SECRET_KEY",
		];
		let _guard = EnvSnapshot::new(&watched);
		let unique = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("system time should be after UNIX_EPOCH")
			.as_nanos();
		let root = env::temp_dir().join(format!("reinhardt-cloud-dotenv-direct-{unique}"));
		let config_dir = root.join("settings");
		write_minimal_local_settings(
			&config_dir,
			"${REINHARDT_CORE__SECRET_KEY:?missing test secret}",
		);
		fs::write(
			root.join(".env.local"),
			"REINHARDT_CORE__SECRET_KEY=dotenv-profile-secret\n",
		)
		.expect("profile dotenv file should be written");

		// SAFETY: `#[serial(env_settings_load)]` provides the cross-test
		// exclusion that `set_var` needs.
		unsafe {
			env::set_var("REINHARDT_CLOUD_CONFIG_DIR", &config_dir);
			env::set_var("REINHARDT_ENV", "local");
			env::set_var("REINHARDT_CORE__SECRET_KEY", "direct-env-secret");
		}

		// Act
		let outcome = try_build_settings();
		let _ = fs::remove_dir_all(&root);

		// Assert
		let settings = outcome.expect("direct env should survive dotenv loading");
		assert_eq!(settings.core.secret_key, "direct-env-secret");
	}

	#[rstest]
	#[serial(env_settings_load)]
	fn get_redis_url_prefers_dotenv_env_over_profile_toml() {
		// Arrange
		let watched: Vec<&'static str> = vec![
			"REINHARDT_CLOUD_CONFIG_DIR",
			"REINHARDT_ENV",
			"REINHARDT_CLOUD_REDIS_URL",
		];
		let _guard = EnvSnapshot::new(&watched);
		let unique = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("system time should be after UNIX_EPOCH")
			.as_nanos();
		let root = env::temp_dir().join(format!("reinhardt-cloud-dotenv-redis-{unique}"));
		let config_dir = root.join("settings");
		fs::create_dir_all(&config_dir).expect("temporary settings directory should be created");
		fs::write(
			config_dir.join("local.toml"),
			r#"redis_url = "redis://toml-redis:6379/0"
"#,
		)
		.expect("local settings file should be written");
		fs::write(
			root.join(".env.local"),
			"REINHARDT_CLOUD_REDIS_URL=redis://dotenv-redis:6379/0\n",
		)
		.expect("profile dotenv file should be written");

		// SAFETY: `#[serial(env_settings_load)]` provides the cross-test
		// exclusion that `set_var`/`remove_var` need.
		unsafe {
			env::set_var("REINHARDT_CLOUD_CONFIG_DIR", &config_dir);
			env::set_var("REINHARDT_ENV", "local");
			env::remove_var("REINHARDT_CLOUD_REDIS_URL");
		}

		// Act
		let redis_url = get_redis_url();
		let _ = fs::remove_dir_all(&root);

		// Assert
		assert_eq!(redis_url.as_deref(), Some("redis://dotenv-redis:6379/0"));
	}

	#[rstest]
	#[serial(env_settings_load)]
	fn test_get_jwt_secret_reads_from_runtime_env() {
		// Arrange — committed settings files must not contain literal JWT
		// secrets, so runtime env is the expected source for local/CI profiles.
		// Issue: #494
		let watched = ["REINHARDT_CLOUD_CONFIG_DIR", "REINHARDT_CLOUD_JWT_SECRET"];
		let _guard = EnvSnapshot::new(&watched);

		// SAFETY: `#[serial(env_settings_load)]` provides the cross-test
		// exclusion that `set_var`/`remove_var` need.
		unsafe {
			env::remove_var("REINHARDT_CLOUD_CONFIG_DIR");
			env::set_var(
				"REINHARDT_CLOUD_JWT_SECRET",
				"runtime-jwt-secret-minimum-32-bytes",
			);
		}

		// Act
		let secret = get_jwt_secret();

		// Assert
		assert_eq!(
			secret.as_deref(),
			Some("runtime-jwt-secret-minimum-32-bytes"),
		);
	}

	#[rstest]
	#[serial(env_settings_load)]
	fn test_get_redis_url_prefers_runtime_env_over_profile_toml() {
		// Arrange — `ci.toml` contains a localhost Redis fallback, but deployed
		// pods receive the real Redis endpoint from the operator at runtime.
		let watched = [
			"REINHARDT_CLOUD_CONFIG_DIR",
			"REINHARDT_ENV",
			"REINHARDT_CLOUD_REDIS_URL",
		];
		let _guard = EnvSnapshot::new(&watched);

		// SAFETY: `#[serial(env_settings_load)]` provides the cross-test
		// exclusion that `set_var`/`remove_var` need.
		unsafe {
			env::remove_var("REINHARDT_CLOUD_CONFIG_DIR");
			env::set_var("REINHARDT_ENV", "ci");
			env::set_var("REINHARDT_CLOUD_REDIS_URL", "redis://operator-redis:6379/0");
		}

		// Act
		let redis_url = get_redis_url();

		// Assert
		assert_eq!(redis_url.as_deref(), Some("redis://operator-redis:6379/0"));
	}

	#[rstest]
	#[serial(env_settings_load)]
	fn test_email_runtime_env_overrides_profile_toml() {
		// Arrange — `ci.toml` uses the console backend for generic unit tests,
		// but Mailpit-backed auth tests and deployed runtimes must be able to
		// override the full email fragment via `REINHARDT_EMAIL__*`.
		let watched = [
			"REINHARDT_CLOUD_CONFIG_DIR",
			"REINHARDT_ENV",
			"REINHARDT_CLOUD_JWT_SECRET",
			"REINHARDT_CORE__SECRET_KEY",
			"REINHARDT_DATABASE_PASSWORD",
			"REINHARDT_EMAIL__BACKEND",
			"REINHARDT_EMAIL__HOST",
			"REINHARDT_EMAIL__PORT",
		];
		let _guard = EnvSnapshot::new(&watched);

		// SAFETY: `#[serial(env_settings_load)]` provides the cross-test
		// exclusion that `set_var`/`remove_var` need.
		unsafe {
			env::remove_var("REINHARDT_CLOUD_CONFIG_DIR");
			env::set_var("REINHARDT_ENV", "ci");
			env::set_var(
				"REINHARDT_CLOUD_JWT_SECRET",
				"ci-test-secret-minimum-32-bytes-long!!",
			);
			env::set_var(
				"REINHARDT_CORE__SECRET_KEY",
				"ci-test-core-secret-key-minimum-32-bytes",
			);
			env::set_var("REINHARDT_DATABASE_PASSWORD", "postgres");
			env::set_var("REINHARDT_EMAIL__BACKEND", "smtp");
			env::set_var("REINHARDT_EMAIL__HOST", "127.0.0.1");
			env::set_var("REINHARDT_EMAIL__PORT", "2525");
		}

		// Act
		let settings = try_build_settings().expect("ci settings should load with email overrides");

		// Assert
		assert_eq!(settings.email.backend, "smtp");
		assert_eq!(settings.email.host, "127.0.0.1");
		assert_eq!(settings.email.port, 2525);
	}

	/// `production.toml` rewritten in #588 must fail-fast at startup if any
	/// required `${VAR:?msg}` env var is missing — otherwise the dashboard
	/// would silently boot with a placeholder secret. The error message must
	/// name the missing env var so an operator can fix the deployment without
	/// guessing.
	#[rstest]
	#[serial(env_settings_load)]
	fn test_production_profile_fails_fast_when_required_env_vars_missing() {
		// Arrange — `EnvSnapshot::drop` restores the original env even if the
		// assertions below panic, keeping the `env_settings_load` group safe.
		let mut watched: Vec<&'static str> = vec!["REINHARDT_ENV"];
		watched.extend_from_slice(DEPLOYED_REQUIRED_ENV_VARS);
		let _guard = EnvSnapshot::new(&watched);

		// SAFETY: `#[serial(env_settings_load)]` provides the cross-test
		// exclusion that `set_var`/`remove_var` need.
		unsafe {
			env::set_var("REINHARDT_ENV", "production");
			for name in DEPLOYED_REQUIRED_ENV_VARS {
				env::remove_var(name);
			}
		}

		// Act
		let outcome = try_build_settings();

		// Assert — error must mention at least one of the required env vars.
		let err = outcome.expect_err(
			"production.toml must reject startup with no env vars set — see \
			 issue #588 for the fail-fast contract",
		);
		let msg = err.to_string();
		let mut full = msg.clone();
		let mut source = std::error::Error::source(&err);
		while let Some(s) = source {
			full.push_str("\nCaused by: ");
			full.push_str(&s.to_string());
			source = s.source();
		}

		assert!(
			DEPLOYED_REQUIRED_ENV_VARS.iter().any(|v| full.contains(v)),
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
		// Arrange — `EnvSnapshot::drop` restores the original env even on panic.
		let watched: Vec<&'static str> = vec![
			"REINHARDT_ENV",
			"REINHARDT_CLOUD_JWT_SECRET",
			"REINHARDT_CLOUD_REDIS_URL",
			"REINHARDT_CORE__SECRET_KEY",
			"REINHARDT_DATABASE_PASSWORD",
			"REINHARDT_DATABASE_HOST",
			"REINHARDT_EMAIL__HOST",
		];
		let _guard = EnvSnapshot::new(&watched);

		// SAFETY: `#[serial(env_settings_load)]` provides the cross-test
		// exclusion that `set_var`/`remove_var` need.
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

		// Assert
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

	#[rstest]
	#[case("production")]
	#[case("staging")]
	#[serial(env_settings_load)]
	fn test_deployed_profile_toml_self_contained_without_base_toml(#[case] profile: &str) {
		// Arrange
		let temp_settings = TempSettingsDir::with_profile(profile);
		let _guard = set_deployed_profile_env(profile, temp_settings.path());
		assert_profile_file_contains_runtime_sections(&temp_settings.profile_path(profile));

		// Act
		let settings =
			try_build_settings().expect("deployed profile should load without base.toml");

		// Assert
		assert_eq!(settings.i18n.language_code, "en-us");
		assert_eq!(settings.i18n.time_zone, "UTC");
		assert!(settings.i18n.use_i18n);
		assert!(settings.i18n.use_tz);
		assert_eq!(settings.static_files.url, "/static/");
		assert_eq!(settings.static_files.root, PathBuf::from("static"));
		assert_eq!(settings.media.url, "/media/");
		assert_eq!(settings.media.root, PathBuf::from("media"));
	}
}
