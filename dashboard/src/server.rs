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

use crate::config::settings::{ProjectSettings, get_resolved_settings};

/// Directory holding the WASM bundle and `index.html` inside the runtime image.
const PAGES_STATIC_DIR: &str = "/app/static/wasm";

/// Build the `runserver` context used by the container entry point.
///
/// Autoreload and the startup WASM build are disabled because the runtime image ships
/// neither a `src/` tree to watch nor a Rust toolchain; the prebuilt Pages
/// bundle is served from [`PAGES_STATIC_DIR`]. The validated project settings
/// are attached to the context; without them `runserver` would build its own
/// settings and skip the startup validation.
fn build_context(bind_addr: &str, settings: ProjectSettings) -> CommandContext {
	let mut options: HashMap<String, Vec<String>> = HashMap::new();
	options.insert("noreload".to_owned(), Vec::new());
	options.insert("with-pages".to_owned(), Vec::new());
	options.insert("no-wasm".to_owned(), Vec::new());
	options.insert("static-dir".to_owned(), vec![PAGES_STATIC_DIR.to_owned()]);
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
fn prepare_context(bind_addr: &str) -> Result<CommandContext, Box<dyn Error>> {
	let settings = get_resolved_settings()?.into_parts().0;
	Ok(build_context(bind_addr, settings))
}

/// Run the HTTP server on `bind_addr` until shutdown.
///
/// # Errors
///
/// Returns an error when settings validation fails, route registration fails,
/// or the server itself fails.
pub async fn run(bind_addr: &str) -> Result<(), Box<dyn Error>> {
	RunServerCommand
		.execute(&prepare_context(bind_addr)?)
		.await?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use rstest::rstest;
	use serial_test::serial;

	use crate::config::settings::test_support::{EnvGuard, required_env};

	use super::{PAGES_STATIC_DIR, prepare_context, run};

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
}
