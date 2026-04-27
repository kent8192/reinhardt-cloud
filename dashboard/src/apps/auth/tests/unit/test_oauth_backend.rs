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

	use crate::apps::auth::services::oauth::backend::build_social_auth_backend;
	use crate::apps::auth::services::oauth::config::{OAuthSettings, ProviderCredentials};

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
}
