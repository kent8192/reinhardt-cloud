//! Tests for `build_social_auth_backend`.
//!
//! These cover the dashboard's wiring of `reinhardt-auth`'s
//! `SocialAuthBackend` — specifically the contract that:
//!   * an empty `OAuthSettings` (no providers configured) returns `None`,
//!     so callers can short-circuit endpoint dispatch instead of building
//!     a backend with no registered providers; and
//!   * a populated `OAuthSettings` returns `Some(Arc<...>)` so views can
//!     immediately call `begin_auth` / `handle_callback`.

#[cfg(test)]
mod tests {
	use rstest::rstest;
	use serial_test::serial;

	use crate::apps::auth::services::oauth::backend::build_social_auth_backend;
	use crate::apps::auth::services::oauth::config::{OAuthSettings, ProviderCredentials};

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
	#[tokio::test]
	async fn test_build_returns_none_when_no_providers_configured() {
		// Arrange
		let settings = OAuthSettings::default();

		// Act
		let result = build_social_auth_backend(&settings)
			.await
			.expect("build should not error");

		// Assert
		assert!(
			result.is_none(),
			"with no providers, factory must return None to let callers short-circuit"
		);
	}

	#[rstest]
	#[tokio::test]
	async fn test_build_returns_backend_when_github_configured() {
		// Arrange
		let settings = OAuthSettings {
			github: Some(ProviderCredentials {
				client_id: "test-id".to_string(),
				client_secret: "test-secret".to_string(),
				redirect_uri: "https://example.test/cb".to_string(),
			}),
		};

		// Act
		let result = build_social_auth_backend(&settings)
			.await
			.expect("build should succeed with valid github creds");

		// Assert
		assert!(
			result.is_some(),
			"with github configured, factory must return a backend"
		);
	}

	#[rstest]
	#[serial(env_oauth_endpoints)]
	#[tokio::test]
	async fn test_build_applies_endpoint_overrides_from_env() {
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
		let settings = populated_settings();

		// Act
		let result = build_social_auth_backend(&settings)
			.await
			.expect("build should succeed with endpoint overrides");

		// Assert
		assert!(
			result.is_some(),
			"endpoint overrides must not disable the provider"
		);
	}

	#[rstest]
	#[serial(env_oauth_endpoints)]
	#[tokio::test]
	async fn test_build_treats_empty_endpoint_override_as_unset() {
		// Arrange — `non_empty_env` filters out empty strings so an
		// exported-but-empty override does not silently replace the
		// canonical github.com endpoint with `""`.
		let _g = EnvGuard::set(vec![
			(KEY_AUTHORIZE, Some("")),
			(KEY_TOKEN, Some("")),
			(KEY_USERINFO, Some("")),
		]);
		let settings = populated_settings();

		// Act
		let result = build_social_auth_backend(&settings)
			.await
			.expect("build should succeed even with empty overrides");

		// Assert
		assert!(
			result.is_some(),
			"empty overrides must not disable the provider"
		);
	}
}
