//! Production HTTP server bootstrap.
//!
//! The `reinhardt-cloud-dashboard` binary is a thin launcher around
//! [`run`]. Everything that registers with the DI container, the router
//! inventory, or the settings system lives in this library crate so that the
//! binary and `manage` observe the same registrations.

use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;

use reinhardt::commands::{BaseCommand, CommandContext, RunServerCommand};
use reinhardt::db::backends::DatabaseConnection;
use reinhardt::db::orm::init_database;

use crate::config::settings::{ProjectSettings, get_resolved_settings, profile_name};

/// Directory holding the WASM bundle and `index.html` inside the runtime image.
const PAGES_STATIC_DIR: &str = "/app/static/wasm";

/// Whether the OpenAPI document, Swagger UI, and ReDoc are served.
///
/// Only the `local` and `ci` profiles serve them (SR-11); every other profile,
/// including one with an unrecognized name, never registers them, so their
/// paths answer like any unknown path. (The framework's own `Profile` enum
/// cannot tell `local` or `ci` from any other custom name, so the profile name
/// is compared directly.)
fn serves_api_documentation(profile: &str) -> bool {
	matches!(profile, "local" | "ci")
}

/// Build the `runserver` context used by the container entry point.
///
/// Autoreload and the startup WASM build are disabled because the runtime image ships
/// neither a `src/` tree to watch nor a Rust toolchain; the prebuilt Pages
/// bundle is served from [`PAGES_STATIC_DIR`]. The validated project settings
/// are attached to the context; without them `runserver` would build its own
/// settings and skip the startup validation. API documentation is switched off
/// outside the `local` and `ci` profiles.
fn build_context(bind_addr: &str, settings: ProjectSettings, profile: &str) -> CommandContext {
	let mut options: HashMap<String, Vec<String>> = HashMap::new();
	options.insert("noreload".to_owned(), Vec::new());
	options.insert("with-pages".to_owned(), Vec::new());
	options.insert("no-wasm".to_owned(), Vec::new());
	options.insert("static-dir".to_owned(), vec![PAGES_STATIC_DIR.to_owned()]);
	if !serves_api_documentation(profile) {
		// `runserver` reads this flag with an underscore, unlike its other
		// hyphenated options.
		options.insert("no_docs".to_owned(), Vec::new());
	}
	CommandContext::new(vec![bind_addr.to_owned()])
		.with_options(options)
		.with_settings(Arc::new(settings))
}

/// Load the validated settings and build the `runserver` context.
///
/// Settings validation (required secrets, the hardened `staging` and
/// `production` profiles, the provider-token key) runs here, before anything
/// listens, so a misconfigured deployment stops with a clear error (SR-99,
/// SR-100, SR-06).
pub(crate) fn prepare_context(bind_addr: &str) -> Result<CommandContext, Box<dyn Error>> {
	let settings = get_resolved_settings()?.into_parts().0;
	Ok(build_context(bind_addr, settings, &profile_name()))
}

/// Initialize the global ORM connection pool from the validated settings.
///
/// `runserver` expects the pool to exist: upstream creates it in
/// `run_command_with_registry` before dispatching the command, a step this
/// entry point skips by calling `RunServerCommand` directly. Without it
/// `runserver` only warns that it cannot register the database connection, and
/// every database-backed handler (`CurrentUser<User>`, the session middleware,
/// the admin site) fails at request time instead of at startup.
async fn initialize_orm_database(ctx: &CommandContext) -> Result<(), Box<dyn Error>> {
	let settings = ctx
		.settings
		.as_deref()
		.ok_or("the server context carries no settings")?;
	let url = DatabaseConnection::database_url_from(settings, None)?;
	init_database(&url).await?;
	Ok(())
}

/// Run the HTTP server on `bind_addr` until shutdown.
///
/// # Errors
///
/// Returns an error when settings validation fails, route registration fails,
/// or the server itself fails.
pub async fn run(bind_addr: &str) -> Result<(), Box<dyn Error>> {
	serve(&prepare_context(bind_addr)?).await
}

/// Run the HTTP server described by `ctx` until shutdown.
///
/// Initializes the ORM pool first (see [`initialize_orm_database`]), then
/// hands the context to `runserver`.
///
/// # Errors
///
/// Returns an error when the database cannot be reached, route registration
/// fails, or the server itself fails.
pub async fn serve(ctx: &CommandContext) -> Result<(), Box<dyn Error>> {
	initialize_orm_database(ctx).await?;
	RunServerCommand.execute(ctx).await?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use rstest::rstest;
	use serial_test::serial;

	use crate::config::settings::test_support::{EnvGuard, required_env};

	use super::{
		PAGES_STATIC_DIR, initialize_orm_database, prepare_context, run, serves_api_documentation,
	};

	#[rstest]
	#[serial(env_settings_load)]
	fn prepare_context_serves_the_prebuilt_pages_bundle_with_validated_settings() {
		// Arrange
		let _env = EnvGuard::apply(&required_env("ci"));
		let bind_addr = "0.0.0.0:8000";

		// Act
		let ctx = prepare_context(bind_addr).expect("valid settings should build a context");

		// Assert
		assert_eq!(ctx.arg(0).map(String::as_str), Some(bind_addr));
		assert!(ctx.has_option("noreload"));
		assert!(ctx.has_option("with-pages"));
		assert!(ctx.has_option("no-wasm"));
		assert_eq!(
			ctx.option("static-dir").map(String::as_str),
			Some(PAGES_STATIC_DIR)
		);
		assert!(
			ctx.settings.is_some(),
			"runserver must receive the validated settings"
		);
	}

	#[rstest]
	#[case::local("local", true)]
	#[case::ci("ci", true)]
	#[case::staging("staging", false)]
	#[case::production("production", false)]
	#[case::development_alias("development", false)]
	#[case::unrecognized("qa", false)]
	#[case::empty("", false)]
	fn sr_11_only_the_local_and_ci_profiles_serve_api_documentation(
		#[case] profile: &str,
		#[case] served: bool,
	) {
		// Arrange / Act
		let result = serves_api_documentation(profile);

		// Assert
		assert_eq!(result, served);
	}

	#[rstest]
	#[serial(env_settings_load)]
	#[case::ci("ci", false)]
	#[case::staging("staging", true)]
	#[case::production("production", true)]
	fn sr_11_the_server_context_withholds_documentation_outside_local_and_ci(
		#[case] profile: &'static str,
		#[case] withheld: bool,
	) {
		// Arrange
		let _env = EnvGuard::apply(&required_env(profile));

		// Act
		let ctx = prepare_context("127.0.0.1:0").expect("valid settings should build a context");

		// Assert
		assert_eq!(ctx.has_option("no_docs"), withheld);
	}

	#[rstest]
	#[serial(database, env_settings_load)]
	#[tokio::test]
	async fn the_server_path_initializes_the_orm_pool_from_the_settings() {
		// Arrange
		let (_container, _pool, port, _url) = reinhardt::test::fixtures::postgres_container().await;
		let port = port.to_string();
		let mut vars = required_env("ci");
		vars.push(("REINHARDT_DATABASE_HOST", Some("localhost")));
		vars.push(("REINHARDT_DATABASE_PORT", Some(port.as_str())));
		vars.push(("REINHARDT_DATABASE_NAME", Some("postgres")));
		vars.push(("REINHARDT_DATABASE_USER", Some("postgres")));
		let _env = EnvGuard::apply(&vars);
		let ctx = prepare_context("127.0.0.1:0").expect("valid settings should build a context");

		// Act
		initialize_orm_database(&ctx)
			.await
			.expect("the pool should come up against the container");

		// Assert
		// Without this step `runserver` cannot register the database connection
		// and every database-backed handler fails at request time.
		let connection = reinhardt::db::orm::get_connection_registration().await;
		assert!(connection.is_ok(), "the ORM pool is initialized");
	}

	#[rstest]
	#[serial(env_settings_load)]
	#[tokio::test]
	async fn the_server_path_refuses_to_start_without_a_reachable_database() {
		// Arrange
		let mut vars = required_env("ci");
		vars.push(("REINHARDT_DATABASE_HOST", Some("127.0.0.1")));
		vars.push(("REINHARDT_DATABASE_PORT", Some("1")));
		let _env = EnvGuard::apply(&vars);
		let ctx = prepare_context("127.0.0.1:0").expect("valid settings should build a context");

		// Act
		let result = initialize_orm_database(&ctx).await;

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	#[serial(env_settings_load)]
	#[case::missing_secret_key("REINHARDT_CORE__SECRET_KEY", None, "REINHARDT_CORE__SECRET_KEY")]
	#[case::empty_database_password(
		"REINHARDT_DATABASE_PASSWORD",
		Some("   "),
		"Validation error: required secret `core.databases.default.password` is empty"
	)]
	#[case::placeholder_redis_url(
		"REINHARDT_CLOUD_REDIS_URL",
		Some("redis://:$(REINHARDT_CLOUD_REDIS_PASSWORD)@redis:6379/0"),
		"Validation error: required secret `redis.url` still holds an unexpanded placeholder"
	)]
	#[case::malformed_token_key(
		"REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY",
		Some("not-a-valid-key"),
		"Validation error: accounts.token_encryption_*: token encryption key must be base64 of exactly 32 bytes"
	)]
	fn sr_99_server_entry_refuses_to_start_with_unusable_settings(
		#[case] variable: &'static str,
		#[case] value: Option<&str>,
		#[case] expected: &str,
	) {
		// Arrange
		let mut vars = required_env("ci");
		vars.push((variable, value));
		let _env = EnvGuard::apply(&vars);

		// Act
		let error = prepare_context("127.0.0.1:0").expect_err("settings must be rejected");

		// Assert
		if value.is_none() {
			// `contains`: a missing variable's message embeds the path of the
			// checkout's `base.toml`, so only the variable name is stable.
			assert!(
				error.to_string().contains(expected),
				"expected `{expected}` in: {error}"
			);
		} else {
			assert_eq!(error.to_string(), expected);
		}
	}

	#[rstest]
	#[serial(env_settings_load)]
	#[tokio::test]
	async fn sr_99_run_fails_before_listening_when_a_required_secret_is_missing() {
		// Arrange
		let mut vars = required_env("ci");
		vars.push(("REINHARDT_CORE__SECRET_KEY", None));
		let _env = EnvGuard::apply(&vars);

		// Act
		let result = run("127.0.0.1:0").await;

		// Assert
		let error = result.expect_err("a missing secret key must stop the server entry");
		// `contains`: the message embeds the checkout's `base.toml` path, so only
		// the variable name is stable across machines.
		assert!(error.to_string().contains("REINHARDT_CORE__SECRET_KEY"));
	}

	#[rstest]
	#[serial(env_settings_load)]
	#[case::staging("staging")]
	#[case::production("production")]
	#[tokio::test]
	async fn run_fails_before_listening_when_a_deployed_profile_has_no_github_app(
		#[case] profile: &'static str,
	) {
		// Arrange
		let mut vars = required_env(profile);
		vars.push(("REINHARDT_CLOUD_GITHUB_CLIENT_ID", None));
		vars.push(("REINHARDT_CLOUD_GITHUB_CLIENT_SECRET", None));
		let _env = EnvGuard::apply(&vars);

		// Act
		let result = run("127.0.0.1:0").await;

		// Assert
		let error = result.expect_err("a deployed profile needs the GitHub App or an opt-out");
		assert_eq!(
			error.to_string(),
			"Validation error: Invalid value for 'accounts.github_client_*': the GitHub App client ID and secret are required in a deployed profile; set `accounts.github_sign_in = \"disabled\"` to run without GitHub sign-in"
		);
	}
}
