//! Tests of settings loading and validation.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use rstest::rstest;
use serial_test::serial;

use reinhardt::conf::settings::builder::BuildError;

use crate::config::settings::test_support::{EnvGuard, TempDir, required_env};
use crate::config::settings::{get_settings, resolve_settings_dir};

#[rstest]
#[serial(env_settings_load)]
fn base_settings_load_secrets_from_the_environment() {
	// Arrange
	let _env = EnvGuard::apply(&required_env("staging"));

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

#[rstest]
#[serial(env_settings_load)]
fn ci_profile_loads_the_tracked_ci_settings() {
	// Arrange
	let _env = EnvGuard::apply(&required_env("ci"));

	// Act
	let settings = get_settings()
		.expect("settings sources should load")
		.resolve()
		.expect("settings should resolve");

	// Assert
	assert_eq!(
		settings.settings().core.allowed_hosts,
		vec!["localhost", "127.0.0.1"]
	);
}

#[rstest]
#[serial(env_settings_load)]
fn settings_fail_fast_when_the_secret_key_is_missing() {
	// Arrange
	let mut vars = required_env("staging");
	vars.push(("REINHARDT_CORE__SECRET_KEY", None));
	let _env = EnvGuard::apply(&vars);

	// Act
	let result = get_settings().and_then(|pending| pending.resolve().map(|_| ()));

	// Assert
	let error = result.expect_err("a missing secret key must fail");
	assert!(
		matches!(error, BuildError::Source { .. }),
		"expected a source error, got: {error}"
	);
	// `contains` instead of `assert_eq!`: the message embeds the absolute path of the
	// checkout's `base.toml`, so only the variable name is stable across machines.
	assert!(
		error.to_string().contains("REINHARDT_CORE__SECRET_KEY"),
		"error should name the missing variable: {error}"
	);
}

#[rstest]
#[serial(env_settings_load)]
fn env_guard_restores_the_original_value_of_a_variable_applied_twice() {
	// Arrange
	const NAME: &str = "REINHARDT_ENV_GUARD_TEST_VARIABLE";
	let _outer = EnvGuard::apply(&[(NAME, Some("original"))]);

	// Act
	{
		let _inner = EnvGuard::apply(&[(NAME, Some("first")), (NAME, Some("second"))]);
		assert_eq!(env::var(NAME).as_deref(), Ok("second"));
	}

	// Assert
	assert_eq!(env::var(NAME).as_deref(), Ok("original"));
}

#[rstest]
#[serial(env_settings_load)]
fn settings_directory_override_replaces_the_default_directory() {
	// Arrange
	let override_dir = TempDir::new("cloud-control-plane-settings");
	let base = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("settings/base.toml"))
		.expect("base.toml should be readable");
	fs::write(override_dir.path().join("base.toml"), base).expect("base.toml should be copied");
	fs::write(
		override_dir.path().join("ci.toml"),
		"[core]\nallowed_hosts = [\"override.example\"]\n",
	)
	.expect("ci.toml should be written");
	let override_path = override_dir
		.path()
		.to_str()
		.expect("temp path is UTF-8")
		.to_owned();
	let mut vars = required_env("ci");
	vars.push(("REINHARDT_CLOUD_CONFIG_DIR", Some(&override_path)));
	let _env = EnvGuard::apply(&vars);

	// Act
	let settings = get_settings()
		.expect("settings sources should load")
		.resolve()
		.expect("settings should resolve");

	// Assert
	assert_eq!(
		settings.settings().core.allowed_hosts,
		vec!["override.example"]
	);
}

#[rstest]
#[serial(env_settings_load)]
fn settings_directory_defaults_to_the_manifest_settings_directory() {
	// Arrange
	let _env = EnvGuard::apply(&[("REINHARDT_CLOUD_CONFIG_DIR", None)]);
	let base_dir = Path::new("/manifest");

	// Act
	let resolved = resolve_settings_dir(base_dir);

	// Assert
	assert_eq!(resolved, base_dir.join("settings"));
}

// ---------------------------------------------------------------------------
// Security requirements: SR-06, SR-19, SR-99, SR-100, SR-101, SR-102
// ---------------------------------------------------------------------------

/// Directory holding the committed settings profiles.
fn committed_settings_dir() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("settings")
}

/// Copy only the committed (tracked) profiles into a fresh directory, so a
/// developer's ignored `local.toml` can never make a test pass by accident.
fn isolated_settings(label: &str, profiles: &[&str]) -> TempDir {
	let dir = TempDir::new(label);
	for profile in profiles {
		let name = format!("{profile}.toml");
		fs::copy(committed_settings_dir().join(&name), dir.path().join(&name))
			.expect("committed profile should be copied");
	}
	dir
}

/// Environment for loading `profile` from `dir` with every required value set.
fn env_for(
	profile: &'static str,
	dir: &TempDir,
	extra: &[(&'static str, Option<&str>)],
) -> EnvGuard {
	let path = dir.path().to_str().expect("temp path is UTF-8").to_owned();
	let mut vars = required_env(profile);
	vars.push(("REINHARDT_CLOUD_CONFIG_DIR", Some(&path)));
	vars.extend_from_slice(extra);
	EnvGuard::apply(&vars)
}

fn load_error(
	profile: &'static str,
	dir: &TempDir,
	extra: &[(&'static str, Option<&str>)],
) -> String {
	let _env = env_for(profile, dir, extra);
	match get_settings() {
		Ok(_) => panic!("settings for `{profile}` should have been rejected"),
		Err(error) => error.to_string(),
	}
}

#[rstest]
#[serial(env_settings_load)]
#[case::staging("staging")]
#[case::production("production")]
fn sr_99_deployed_profiles_are_self_contained_without_a_local_profile(
	#[case] profile: &'static str,
) {
	// Arrange
	let dir = isolated_settings("sr99-self-contained", &["base", profile]);
	let _env = env_for(profile, &dir, &[]);

	// Act
	let settings = get_settings()
		.expect("committed profile should load")
		.resolve()
		.expect("committed profile should resolve");

	// Assert
	assert_eq!(
		settings.settings().core.secret_key,
		"test-only-secret-key-not-for-deployment"
	);
	assert!(!settings.settings().core.allowed_hosts.is_empty());
}

#[rstest]
#[serial(env_settings_load)]
#[case::secret_key("REINHARDT_CORE__SECRET_KEY")]
#[case::database_password("REINHARDT_DATABASE_PASSWORD")]
#[case::redis_url("REINHARDT_CLOUD_REDIS_URL")]
fn sr_99_deployed_profile_fails_fast_when_a_required_secret_is_missing(
	#[case] variable: &'static str,
) {
	// Arrange
	let dir = isolated_settings("sr99-missing", &["base", "production"]);

	// Act
	let message = load_error("production", &dir, &[(variable, None)]);

	// Assert
	// `contains`: the message also embeds the path of the checkout's
	// `base.toml`, so only the variable name is stable across machines.
	assert!(
		message.contains(variable),
		"the error should name the missing variable `{variable}`: {message}"
	);
}

#[rstest]
#[serial(env_settings_load)]
#[case::secret_key("REINHARDT_CORE__SECRET_KEY", "core.secret_key")]
#[case::database_password("REINHARDT_DATABASE_PASSWORD", "core.databases.default.password")]
#[case::redis_url("REINHARDT_CLOUD_REDIS_URL", "redis.url")]
fn sr_99_empty_secret_is_rejected_and_named_without_its_value(
	#[case] variable: &'static str,
	#[case] setting: &str,
) {
	// Arrange
	let dir = isolated_settings("sr99-empty", &["base", "production"]);

	// Act
	let message = load_error("production", &dir, &[(variable, Some("   "))]);

	// Assert
	assert_eq!(
		message,
		format!("Validation error: required secret `{setting}` is empty")
	);
}

#[rstest]
#[serial(env_settings_load)]
#[case::secret_key(
	"REINHARDT_CORE__SECRET_KEY",
	"core.secret_key",
	"${REINHARDT_CORE__SECRET_KEY}"
)]
#[case::database_password(
	"REINHARDT_DATABASE_PASSWORD",
	"core.databases.default.password",
	"${REINHARDT_DATABASE_PASSWORD}"
)]
#[case::redis_url(
	"REINHARDT_CLOUD_REDIS_URL",
	"redis.url",
	"redis://:$(REINHARDT_CLOUD_REDIS_PASSWORD)@web-redis:6379/0"
)]
fn sr_99_unexpanded_placeholder_is_never_used_as_a_secret(
	#[case] variable: &'static str,
	#[case] setting: &str,
	#[case] placeholder: &str,
) {
	// Arrange
	let dir = isolated_settings("sr99-placeholder", &["base", "production"]);

	// Act
	let message = load_error("production", &dir, &[(variable, Some(placeholder))]);

	// Assert
	assert_eq!(
		message,
		format!(
			"Validation error: required secret `{setting}` still holds an unexpanded placeholder"
		)
	);
	assert!(
		!message.contains(placeholder),
		"the error must not repeat the value: {message}"
	);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_99_numeric_setting_cannot_carry_a_placeholder_through_to_use() {
	// Arrange
	let dir = isolated_settings("sr99-numeric", &["base", "production"]);
	let _env = env_for(
		"production",
		&dir,
		&[(
			"REINHARDT_DATABASE_PORT",
			Some("${REINHARDT_DATABASE_PASSWORD}"),
		)],
	);

	// Act
	let result = get_settings().and_then(|pending| pending.resolve().map(|_| ()));

	// Assert
	let message = result
		.expect_err("a placeholder in a numeric setting must not load")
		.to_string();
	assert!(
		!message.contains("test-only-database-password"),
		"the placeholder must not have been expanded into a secret: {message}"
	);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_99_deployed_profile_rejects_a_short_secret_key() {
	// Arrange
	let dir = isolated_settings("sr99-short", &["base", "production"]);

	// Act
	let message = load_error(
		"production",
		&dir,
		&[("REINHARDT_CORE__SECRET_KEY", Some("too-short"))],
	);

	// Assert
	assert_eq!(
		message,
		"Validation error: profile is not hardened: core.secret_key must be at least 32 bytes"
	);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_06_malformed_token_encryption_key_stops_startup() {
	// Arrange
	let dir = isolated_settings("sr06-malformed", &["base", "production"]);

	// Act
	let message = load_error(
		"production",
		&dir,
		&[(
			"REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY",
			Some("not-a-valid-key"),
		)],
	);

	// Assert
	assert_eq!(
		message,
		"Validation error: accounts.token_encryption_*: token encryption key must be base64 of exactly 32 bytes"
	);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_06_token_key_placeholder_is_rejected() {
	// Arrange
	let dir = isolated_settings("sr06-placeholder", &["base", "production"]);

	// Act
	let message = load_error(
		"production",
		&dir,
		&[(
			"REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY",
			Some("${REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY}"),
		)],
	);

	// Assert
	assert_eq!(
		message,
		"Validation error: required secret `accounts.token_encryption_key` still holds an unexpanded placeholder"
	);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_06_token_key_falls_back_to_a_key_derived_from_the_secret_key() {
	// Arrange
	let dir = isolated_settings("sr06-derived", &["base", "production"]);
	let _env = env_for("production", &dir, &[]);

	// Act
	let settings = get_settings().unwrap().resolve().unwrap();
	let keyring = settings
		.settings()
		.accounts
		.token_keyring(&settings.settings().core.secret_key)
		.unwrap();

	// Assert
	assert!(keyring.active_key_id().starts_with("core-"));
}

#[rstest]
#[serial(env_settings_load)]
fn sr_06_dedicated_token_key_from_the_environment_is_used() {
	// Arrange
	let dir = isolated_settings("sr06-dedicated", &["base", "production"]);
	let _env = env_for(
		"production",
		&dir,
		&[
			(
				"REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY",
				Some("BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc="),
			),
			("REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY_ID", Some("2026")),
		],
	);

	// Act
	let settings = get_settings().unwrap().resolve().unwrap();
	let keyring = settings
		.settings()
		.accounts
		.token_keyring(&settings.settings().core.secret_key)
		.unwrap();

	// Assert
	assert_eq!(keyring.active_key_id(), "2026");
}

#[rstest]
#[serial(env_settings_load)]
fn sr_100_production_profile_is_hardened_by_default() {
	// Arrange
	let dir = isolated_settings("sr100-hardened", &["base", "production"]);
	let _env = env_for("production", &dir, &[]);

	// Act
	let pending = get_settings().expect("production should load");
	let settings = pending.resolve().unwrap();
	let core = &settings.settings().core;
	let origins = pending
		.deserialize_section::<serde_json::Value>("ws_origin")
		.unwrap();

	// Assert
	assert!(!core.debug);
	assert!(core.security.session_cookie_secure);
	assert!(core.security.csrf_cookie_secure);
	assert!(core.security.secure_ssl_redirect);
	assert_eq!(core.security.secure_hsts_seconds, Some(31_536_000));
	assert_eq!(
		core.allowed_hosts,
		vec!["reinhardt-cloud.dev", "www.reinhardt-cloud.dev"]
	);
	assert_eq!(
		origins["policy"]["origins"],
		serde_json::json!([
			"https://reinhardt-cloud.dev",
			"https://www.reinhardt-cloud.dev"
		])
	);
}

#[rstest]
#[serial(env_settings_load)]
#[case::debug_on("[core]\ndebug = true\n", "core.debug must be false")]
#[case::insecure_session_cookie(
	"[core.security]\nsession_cookie_secure = false\n",
	"core.security.session_cookie_secure must be true"
)]
#[case::insecure_csrf_cookie(
	"[core.security]\ncsrf_cookie_secure = false\n",
	"core.security.csrf_cookie_secure must be true"
)]
#[case::no_https_redirect(
	"[core.security]\nsecure_ssl_redirect = false\n",
	"core.security.secure_ssl_redirect must be true"
)]
#[case::no_hsts(
	"[core.security]\nsecure_hsts_seconds = 0\n",
	"core.security.secure_hsts_seconds must be greater than zero"
)]
#[case::no_hosts(
	"[core]\nallowed_hosts = []\n",
	"core.allowed_hosts must list hosts explicitly"
)]
#[case::wildcard_host(
	"[core]\nallowed_hosts = [\"*\"]\n",
	"core.allowed_hosts must not contain wildcard or localhost entries"
)]
#[case::wildcard_subdomain_host(
	"[core]\nallowed_hosts = [\".reinhardt-cloud.dev\"]\n",
	"core.allowed_hosts must not contain wildcard or localhost entries"
)]
#[case::localhost_host(
	"[core]\nallowed_hosts = [\"localhost\"]\n",
	"core.allowed_hosts must not contain wildcard or localhost entries"
)]
#[case::localhost_origin(
	"[ws_origin]\npolicy = { mode = \"allow_list\", origins = [\"https://localhost:8000\"] }\n",
	"ws_origin.policy.origins must be https origins without wildcard or localhost entries"
)]
#[case::plaintext_origin(
	"[ws_origin]\npolicy = { mode = \"allow_list\", origins = [\"http://reinhardt-cloud.dev\"] }\n",
	"ws_origin.policy.origins must be https origins without wildcard or localhost entries"
)]
fn sr_100_an_unhardened_deployed_profile_refuses_to_start(
	#[case] override_toml: &str,
	#[case] expected: &str,
) {
	// Arrange
	let dir = isolated_settings("sr100-unhardened", &["base", "staging"]);
	let staging = fs::read_to_string(dir.path().join("staging.toml")).unwrap();
	let mut table: toml::Table = staging.parse().unwrap();
	let overrides: toml::Table = override_toml.parse().unwrap();
	merge_tables(&mut table, overrides);
	fs::write(dir.path().join("staging.toml"), table.to_string()).unwrap();

	// Act
	let message = load_error("staging", &dir, &[]);

	// Assert
	assert_eq!(
		message,
		format!("Validation error: profile is not hardened: {expected}")
	);
}

/// Deep-merge `overrides` into `target`.
fn merge_tables(target: &mut toml::Table, overrides: toml::Table) {
	for (key, value) in overrides {
		match (target.get_mut(&key), value) {
			(Some(toml::Value::Table(existing)), toml::Value::Table(incoming)) => {
				merge_tables(existing, incoming);
			}
			(_, value) => {
				target.insert(key, value);
			}
		}
	}
}

#[rstest]
#[serial(env_settings_load)]
fn sr_100_production_refuses_to_start_in_debug_mode() {
	// Arrange
	let dir = isolated_settings("sr100-debug", &["base", "production"]);
	let production = fs::read_to_string(dir.path().join("production.toml")).unwrap();
	fs::write(
		dir.path().join("production.toml"),
		production.replace("debug = false", "debug = true"),
	)
	.unwrap();

	// Act
	let message = load_error("production", &dir, &[]);

	// Assert
	assert_eq!(
		message,
		"Validation error: Security error: debug must be false in production"
	);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_100_staging_profile_is_hardened_too() {
	// Arrange
	let dir = isolated_settings("sr100-staging", &["base", "staging"]);
	let _env = env_for("staging", &dir, &[]);

	// Act
	let settings = get_settings()
		.expect("staging should load")
		.resolve()
		.unwrap();
	let core = &settings.settings().core;

	// Assert
	assert!(!core.debug);
	assert!(core.security.session_cookie_secure);
	assert!(core.security.secure_ssl_redirect);
	assert_eq!(core.allowed_hosts, vec!["staging.reinhardt-cloud.dev"]);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_100_development_conveniences_exist_only_in_the_ignored_local_profile() {
	// Arrange
	let committed = ["base", "ci", "staging", "production"];

	// Act
	let offenders: Vec<_> = committed
		.iter()
		.filter(|name| {
			let text =
				fs::read_to_string(committed_settings_dir().join(format!("{name}.toml"))).unwrap();
			let table: toml::Table = text.parse().unwrap();
			table
				.get("core")
				.and_then(|core| core.get("debug"))
				.and_then(toml::Value::as_bool)
				== Some(true)
		})
		.collect();

	// Assert
	assert_eq!(offenders, Vec::<&&str>::new());
}

/// Keys whose string values are secrets and so must come from the environment.
fn is_secret_key_name(key: &str) -> bool {
	[
		"secret",
		"password",
		"token_encryption",
		"private_key",
		"api_key",
		"url",
	]
	.iter()
	.any(|needle| key.contains(needle))
}

fn collect_literal_secrets(path: &str, value: &toml::Value, found: &mut Vec<String>) {
	match value {
		toml::Value::Table(table) => {
			for (key, child) in table {
				let child_path = format!("{path}.{key}");
				if let (true, toml::Value::String(text)) = (is_secret_key_name(key), child) {
					// A reference (`${VAR...}`) or an empty value is allowed; a
					// literal is not. A URL without credentials (`[static] url`,
					// a local Redis URL) is not a secret.
					let is_reference = text.is_empty() || text.starts_with("${");
					let is_public_url = key == "url" && !text.contains('@');
					if !is_reference && !is_public_url {
						found.push(child_path.clone());
					}
				}
				collect_literal_secrets(&child_path, child, found);
			}
		}
		toml::Value::Array(items) => {
			for item in items {
				collect_literal_secrets(path, item, found);
			}
		}
		_ => {}
	}
}

#[rstest]
#[case("base.toml")]
#[case("ci.toml")]
#[case("staging.toml")]
#[case("production.toml")]
#[case("local.example.toml")]
fn sr_101_committed_settings_hold_no_literal_secrets(#[case] file: &str) {
	// Arrange
	let text = fs::read_to_string(committed_settings_dir().join(file)).unwrap();
	let table = toml::Value::Table(text.parse().unwrap());

	// Act
	let mut found = Vec::new();
	collect_literal_secrets("", &table, &mut found);

	// Assert
	assert_eq!(found, Vec::<String>::new());
}

#[rstest]
fn sr_101_devcontainer_compose_file_holds_no_literal_password() {
	// Arrange
	let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../.devcontainer/docker-compose.yml");
	let text = fs::read_to_string(path).unwrap();

	// Act
	let literal_passwords: Vec<_> = text
		.lines()
		.map(str::trim)
		.filter(|line| line.contains("PASSWORD") && !line.starts_with('#'))
		.filter(|line| !line.contains("${"))
		.collect();

	// Assert
	assert_eq!(literal_passwords, Vec::<&str>::new());
}

#[rstest]
#[serial(env_settings_load)]
fn sr_101_operator_injected_environment_feeds_the_settings() {
	// Arrange
	let dir = isolated_settings("sr101-operator", &["base", "production"]);
	let _env = env_for(
		"production",
		&dir,
		&[
			("REINHARDT_DATABASE_HOST", Some("db.internal")),
			("REINHARDT_DATABASE_PORT", Some("6543")),
			("REINHARDT_DATABASE_NAME", Some("control_plane")),
			("REINHARDT_DATABASE_USER", Some("cp_user")),
			(
				"REINHARDT_CLOUD_REDIS_URL",
				Some("redis://:injected-password@web-redis:6379/0"),
			),
		],
	);

	// Act
	let settings = get_settings().unwrap().resolve().unwrap();
	let database = &settings.settings().core.databases["default"];

	// Assert
	assert_eq!(database.host.as_deref(), Some("db.internal"));
	assert_eq!(database.port, Some(6543));
	assert_eq!(database.name, "control_plane");
	assert_eq!(database.user.as_deref(), Some("cp_user"));
	assert_eq!(
		database
			.password
			.as_ref()
			.map(|p| p.expose_secret().to_owned())
			.as_deref(),
		Some("test-only-database-password")
	);
	assert_eq!(
		settings.settings().redis.url.expose_secret(),
		"redis://:injected-password@web-redis:6379/0"
	);
	assert_eq!(
		settings.settings().core.secret_key,
		"test-only-secret-key-not-for-deployment"
	);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_102_settings_debug_output_redacts_secrets() {
	// Arrange
	let dir = isolated_settings("sr102-debug", &["base", "production"]);
	let _env = env_for(
		"production",
		&dir,
		&[(
			"REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY",
			Some("BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc="),
		)],
	);
	let settings = get_settings().unwrap().resolve().unwrap();

	// Act
	let rendered = format!(
		"{:?} {:?}",
		settings.settings().redis,
		settings.settings().accounts
	);
	let database = format!("{:?}", settings.settings().core.databases["default"]);

	// Assert
	for secret in [
		"test-redis-password",
		"BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=",
	] {
		// No message: formatting the rendered settings into a failure message
		// would write them to the test log.
		assert!(!rendered.contains(secret));
	}
	assert!(!database.contains("test-only-database-password"));
	assert!(rendered.contains("SecretString([REDACTED])"));
}

#[rstest]
#[serial(env_settings_load)]
fn sr_19_sign_up_policy_defaults_to_invite_only() {
	// Arrange
	let dir = isolated_settings("sr19-default", &["base", "production"]);
	let _env = env_for("production", &dir, &[]);

	// Act
	let settings = get_settings().unwrap().resolve().unwrap();
	let policy = settings.settings().accounts.sign_up_policy().unwrap();

	// Assert
	assert_eq!(
		policy,
		crate::apps::accounts::services::server::sign_up_policy::SignUpPolicy::InviteOnly
	);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_19_sign_up_policy_is_read_from_the_environment() {
	// Arrange
	let dir = isolated_settings("sr19-env", &["base", "production"]);
	let _env = env_for(
		"production",
		&dir,
		&[
			("REINHARDT_CLOUD_SIGN_UP_POLICY", Some("allowlist")),
			(
				"REINHARDT_CLOUD_SIGN_UP_ALLOWED_GITHUB_USER_IDS",
				Some("7, 8"),
			),
			(
				"REINHARDT_CLOUD_SIGN_UP_ALLOWED_GITHUB_ORGANIZATION_IDS",
				Some("100"),
			),
		],
	);

	// Act
	let settings = get_settings().unwrap().resolve().unwrap();
	let policy = settings.settings().accounts.sign_up_policy().unwrap();

	// Assert
	assert_eq!(
		policy,
		crate::apps::accounts::services::server::sign_up_policy::SignUpPolicy::Allowlist {
			user_ids: [7, 8].into(),
			organization_ids: [100].into(),
		}
	);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_19_an_unrecognized_sign_up_policy_resolves_to_invite_only() {
	// Arrange
	let dir = isolated_settings("sr19-unknown", &["base", "production"]);
	let _env = env_for(
		"production",
		&dir,
		&[("REINHARDT_CLOUD_SIGN_UP_POLICY", Some("anybody-at-all"))],
	);

	// Act
	let settings = get_settings().unwrap().resolve().unwrap();
	let policy = settings.settings().accounts.sign_up_policy().unwrap();

	// Assert
	assert_eq!(
		policy,
		crate::apps::accounts::services::server::sign_up_policy::SignUpPolicy::InviteOnly
	);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_19_a_malformed_allowlist_entry_stops_startup() {
	// Arrange
	let dir = isolated_settings("sr19-malformed", &["base", "production"]);

	// Act
	let message = load_error(
		"production",
		&dir,
		&[(
			"REINHARDT_CLOUD_SIGN_UP_ALLOWED_GITHUB_ORGANIZATION_IDS",
			Some("my-org"),
		)],
	);

	// Assert
	assert_eq!(
		message,
		"Validation error: Invalid value for 'accounts.sign_up_allowed_*': sign-up allowlist entry \"my-org\" is not a numeric GitHub ID"
	);
}

#[rstest]
#[serial(env_settings_load)]
#[case::id_without_secret(Some("Iv1.abc"), None)]
#[case::secret_without_id(None, Some("s3cret"))]
fn a_half_configured_github_app_stops_startup(
	#[case] client_id: Option<&'static str>,
	#[case] client_secret: Option<&'static str>,
) {
	// Arrange
	let dir = isolated_settings("github-half", &["base", "production"]);

	// Act
	let message = load_error(
		"production",
		&dir,
		&[
			("REINHARDT_CLOUD_GITHUB_CLIENT_ID", client_id),
			("REINHARDT_CLOUD_GITHUB_CLIENT_SECRET", client_secret),
		],
	);

	// Assert
	assert_eq!(
		message,
		"Validation error: Invalid value for 'accounts.github_client_*': the GitHub App client ID and secret must be set together"
	);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_99_a_placeholder_is_never_used_as_the_github_client_secret() {
	// Arrange
	let dir = isolated_settings("github-placeholder", &["base", "production"]);

	// Act
	let message = load_error(
		"production",
		&dir,
		&[
			("REINHARDT_CLOUD_GITHUB_CLIENT_ID", Some("Iv1.abc")),
			(
				"REINHARDT_CLOUD_GITHUB_CLIENT_SECRET",
				Some("$(GITHUB_CLIENT_SECRET)"),
			),
		],
	);

	// Assert
	assert_eq!(
		message,
		"Validation error: required secret `accounts.github_client_secret` still holds an unexpanded placeholder"
	);
}

#[rstest]
#[serial(env_settings_load)]
fn a_deployed_profile_with_a_github_app_requires_an_https_public_url() {
	// Arrange
	let dir = isolated_settings("github-https", &["base", "production"]);

	// Act
	let message = load_error(
		"production",
		&dir,
		&[
			("REINHARDT_CLOUD_GITHUB_CLIENT_ID", Some("Iv1.abc")),
			("REINHARDT_CLOUD_GITHUB_CLIENT_SECRET", Some("s3cret")),
			(
				"REINHARDT_CLOUD_PUBLIC_URL",
				Some("http://reinhardt-cloud.dev"),
			),
		],
	);

	// Assert
	assert_eq!(
		message,
		"Validation error: Invalid value for 'accounts.public_url': must be an https origin in a deployed profile"
	);
}

#[rstest]
#[serial(env_settings_load)]
#[case::staging("staging", "https://staging.reinhardt-cloud.dev")]
#[case::production("production", "https://reinhardt-cloud.dev")]
fn the_deployed_profiles_name_their_https_public_url_and_the_callback_derives_from_it(
	#[case] profile: &'static str,
	#[case] origin: &str,
) {
	// Arrange
	let dir = isolated_settings("github-public-url", &["base", profile]);
	let _env = env_for(
		profile,
		&dir,
		&[
			("REINHARDT_CLOUD_GITHUB_CLIENT_ID", Some("Iv1.abc")),
			("REINHARDT_CLOUD_GITHUB_CLIENT_SECRET", Some("s3cret")),
		],
	);

	// Act
	let settings = get_settings().unwrap().resolve().unwrap();
	let accounts = &settings.settings().accounts;
	let app = accounts.github_app().expect("the App is configured");

	// Assert
	assert_eq!(
		app.redirect_uri,
		format!("{origin}/api/auth/github/callback/")
	);
	assert_eq!(accounts.request_origins(false, 8000), [origin]);
}

#[rstest]
#[serial(env_settings_load)]
#[case::staging("staging")]
#[case::production("production")]
fn a_deployed_profile_without_the_github_app_stops_startup_unless_sign_in_is_disabled(
	#[case] profile: &'static str,
) {
	// Arrange
	let dir = isolated_settings("github-required", &["base", profile]);

	// Act
	let message = load_error(
		profile,
		&dir,
		&[
			("REINHARDT_CLOUD_GITHUB_CLIENT_ID", None),
			("REINHARDT_CLOUD_GITHUB_CLIENT_SECRET", None),
		],
	);

	// Assert
	assert_eq!(
		message,
		"Validation error: Invalid value for 'accounts.github_client_*': the GitHub App client ID and secret are required in a deployed profile; set `accounts.github_sign_in = \"disabled\"` to run without GitHub sign-in"
	);
}

#[rstest]
#[serial(env_settings_load)]
#[case::staging("staging")]
#[case::production("production")]
fn sign_in_can_be_disabled_explicitly_so_that_a_login_link_alone_can_run_a_control_plane(
	#[case] profile: &'static str,
) {
	// Arrange
	let dir = isolated_settings("github-disabled", &["base", profile]);
	let _env = env_for(
		profile,
		&dir,
		&[
			("REINHARDT_CLOUD_GITHUB_SIGN_IN", Some("disabled")),
			("REINHARDT_CLOUD_GITHUB_CLIENT_ID", None),
			("REINHARDT_CLOUD_GITHUB_CLIENT_SECRET", None),
		],
	);

	// Act
	let settings = get_settings().unwrap().resolve().unwrap();

	// Assert
	assert!(settings.settings().accounts.github_app().is_none());
}

#[rstest]
#[serial(env_settings_load)]
fn disabling_sign_in_wins_over_a_configured_github_app() {
	// Arrange
	let dir = isolated_settings("github-disabled-wins", &["base", "production"]);
	let _env = env_for(
		"production",
		&dir,
		&[("REINHARDT_CLOUD_GITHUB_SIGN_IN", Some("disabled"))],
	);

	// Act
	let settings = get_settings().unwrap().resolve().unwrap();

	// Assert
	assert!(settings.settings().accounts.github_app().is_none());
}

#[rstest]
#[serial(env_settings_load)]
fn an_unknown_github_sign_in_value_stops_startup() {
	// Arrange
	let dir = isolated_settings("github-sign-in-typo", &["base", "production"]);

	// Act
	let message = load_error(
		"production",
		&dir,
		&[("REINHARDT_CLOUD_GITHUB_SIGN_IN", Some("disable"))],
	);

	// Assert
	assert_eq!(
		message,
		"Validation error: Invalid value for 'accounts.github_sign_in': must be empty or `disabled`"
	);
}

#[rstest]
#[serial(env_settings_load)]
#[case::with_a_path("https://reinhardt-cloud.dev/dashboard")]
#[case::without_a_scheme("reinhardt-cloud.dev")]
fn a_configured_github_app_requires_a_bare_origin_as_the_public_url(#[case] public_url: &str) {
	// Arrange
	let dir = isolated_settings("github-origin", &["base", "production"]);

	// Act
	let message = load_error(
		"production",
		&dir,
		&[("REINHARDT_CLOUD_PUBLIC_URL", Some(public_url))],
	);

	// Assert
	assert_eq!(
		message,
		"Validation error: Invalid value for 'accounts.public_url': must be a non-empty origin without a path (for example `https://host`) when the GitHub App is configured"
	);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_102_the_github_client_secret_is_redacted_in_debug_output() {
	// Arrange
	let dir = isolated_settings("github-redacted", &["base", "production"]);
	let _env = env_for(
		"production",
		&dir,
		&[
			("REINHARDT_CLOUD_GITHUB_CLIENT_ID", Some("Iv1.abc")),
			(
				"REINHARDT_CLOUD_GITHUB_CLIENT_SECRET",
				Some("a-very-secret-github-value"),
			),
		],
	);
	let settings = get_settings().unwrap().resolve().unwrap();

	// Act
	let rendered = format!("{:?}", settings.settings().accounts);

	// Assert
	assert!(!rendered.contains("a-very-secret-github-value"));
}

#[rstest]
#[serial(env_settings_load)]
fn the_ci_profile_names_a_public_url_and_allows_the_documented_loopback_origin() {
	// Arrange
	let dir = isolated_settings("ci-origins", &["base", "ci"]);
	let _env = env_for("ci", &dir, &[]);

	// Act
	let settings = get_settings().unwrap().resolve().unwrap();
	let origins = settings.settings().accounts.request_origins(false, 8000);

	// Assert
	assert_eq!(origins, ["http://127.0.0.1:8000", "http://localhost:8000"]);
}
