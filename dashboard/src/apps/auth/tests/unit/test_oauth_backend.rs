//! Tests for the `OAuthBackendBox` DI factory.
//!
//! Covers the dashboard's wiring of `reinhardt-auth`'s
//! `SocialAuthBackend` through `#[injectable_factory]` — specifically
//! the contract that:
//!   * an empty `OAuthSettings` (no providers configured) resolves to
//!     `OAuthBackendBox(None)`, so callers can short-circuit endpoint
//!     dispatch instead of building a backend with no registered
//!     providers; and
//!   * a populated `OAuthSettings` resolves to `OAuthBackendBox(Some(_))`
//!     so views can immediately call `begin_auth` / `handle_callback`.
//!
//! The no-providers / github-configured branches are also exercised by
//! the inline factory tests in `services/oauth/backend.rs`; this module
//! additionally pins the `REINHARDT_CLOUD_OAUTH_GITHUB_{AUTHORIZE,TOKEN,
//! USERINFO}_URL` env-var override path that the inline tests omit.

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use rstest::rstest;
	use serial_test::serial;

	use crate::apps::auth::services::oauth::backend::OAuthBackendBox;
	use crate::apps::auth::services::oauth::config::{OAuthSettings, ProviderCredentials};
	use crate::config::test_helpers::make_test_di_context;

	const KEY_AUTHORIZE: &str = "REINHARDT_CLOUD_OAUTH_GITHUB_AUTHORIZE_URL";
	const KEY_TOKEN: &str = "REINHARDT_CLOUD_OAUTH_GITHUB_TOKEN_URL";
	const KEY_USERINFO: &str = "REINHARDT_CLOUD_OAUTH_GITHUB_USERINFO_URL";

	/// RAII guard that restores OAuth endpoint env vars on drop.
	struct EnvGuard {
		saved: Vec<(String, Option<String>)>,
	}

	impl EnvGuard {
		fn set(vars: Vec<(&str, Option<&str>)>) -> Self {
			let mut saved = Vec::new();
			for (key, new_val) in &vars {
				saved.push((key.to_string(), std::env::var(key).ok()));
				// SAFETY: called in a serial test before any parallel tasks read these vars.
				unsafe {
					match new_val {
						Some(v) => std::env::set_var(key, v),
						None => std::env::remove_var(key),
					}
				}
			}
			Self { saved }
		}
	}

	impl Drop for EnvGuard {
		fn drop(&mut self) {
			for (key, old_val) in &self.saved {
				// SAFETY: restoring env vars in serial test teardown.
				unsafe {
					match old_val {
						Some(v) => std::env::set_var(key, v),
						None => std::env::remove_var(key),
					}
				}
			}
		}
	}

	fn populated_settings() -> OAuthSettings {
		OAuthSettings {
			github: Some(ProviderCredentials {
				client_id: "test-id".to_string(),
				client_secret: "test-secret".to_string(),
				redirect_uri: "https://example.test/cb".to_string(),
			}),
		}
	}

	#[rstest]
	#[serial(env_oauth_endpoints)]
	#[tokio::test]
	async fn test_factory_applies_endpoint_overrides_from_env() {
		// Arrange — point the GitHub provider at a wiremock-style fake.
		// The factory must accept these without error; the actual
		// endpoint substitution is exercised end-to-end in the e2e
		// suite. Here we only pin that the override path is reachable
		// (covers the `Some(v)` arms inside `github_provider_config`).
		let _g = EnvGuard::set(vec![
			(KEY_AUTHORIZE, Some("https://fake.test/authorize")),
			(KEY_TOKEN, Some("https://fake.test/token")),
			(KEY_USERINFO, Some("https://fake.test/userinfo")),
		]);
		let ctx = make_test_di_context(|scope| {
			scope.set(populated_settings());
		});

		// Act
		let backend: Arc<OAuthBackendBox> = ctx
			.resolve::<OAuthBackendBox>()
			.await
			.expect("factory should resolve when github is configured");

		// Assert
		assert!(
			backend.0.is_some(),
			"endpoint overrides must not disable the provider"
		);
	}

	#[rstest]
	#[serial(env_oauth_endpoints)]
	#[tokio::test]
	async fn test_factory_treats_empty_endpoint_override_as_unset() {
		// Arrange — `non_empty_env` filters out empty strings so an
		// exported-but-empty override does not silently replace the
		// canonical github.com endpoint with `""`.
		let _g = EnvGuard::set(vec![
			(KEY_AUTHORIZE, Some("")),
			(KEY_TOKEN, Some("")),
			(KEY_USERINFO, Some("")),
		]);
		let ctx = make_test_di_context(|scope| {
			scope.set(populated_settings());
		});

		// Act
		let backend: Arc<OAuthBackendBox> = ctx
			.resolve::<OAuthBackendBox>()
			.await
			.expect("factory should resolve even with empty endpoint overrides");

		// Assert
		assert!(
			backend.0.is_some(),
			"empty overrides must not disable the provider"
		);
	}
}
