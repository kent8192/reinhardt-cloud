//! Construction of the framework `SocialAuthBackend`.
//!
//! Wires together:
//!   * the configured providers (today: `GitHubProvider`, see #428); and
//!   * a Redis-backed `AsyncSessionStateStore` for OAuth state and PKCE verifiers.
//!
//! Exposes [`OAuthBackendBox`] (newtype around `Option<Arc<SocialAuthBackend>>`)
//! resolved via `#[injectable]`.
//!
//! OAuth state and PKCE verifiers are stored in Redis through the framework's
//! `AsyncSessionStateStore`, so any dashboard replica can complete a flow.
//! Contextual callbacks atomically consume state and validate the initiating
//! browser/session binding before exchanging provider credentials.
//!
//! ## Test-only endpoint overrides
//!
//! When integration tests point the dashboard at a wiremock-rs server,
//! `REINHARDT_CLOUD_OAUTH_GITHUB_AUTHORIZE_URL`,
//! `REINHARDT_CLOUD_OAUTH_GITHUB_TOKEN_URL`, and
//! `REINHARDT_CLOUD_OAUTH_GITHUB_USERINFO_URL` (when set and non-empty)
//! replace the corresponding GitHub URLs in `ProviderConfig::github`'s
//! `OAuth2Config`. This is the only sanctioned way to redirect a flow
//! at a fake provider without forking `reinhardt-auth`. The overrides
//! are read once when the singleton factory resolves; tests that need
//! to vary endpoints across runs must construct the backend manually
//! and override the scope entry rather than mutating env vars.

use std::env;
use std::sync::Arc;

use reinhardt::RedisSessionBackend;
use reinhardt::auth::social::backend::SocialAuthBackend;
use reinhardt::auth::social::core::config::ProviderConfig;
use reinhardt::auth::social::core::error::SocialAuthError;
use reinhardt::auth::social::providers::github::GitHubProvider;
use reinhardt::di::{Depends, injectable};
use reinhardt::middleware::session::AsyncSessionStateStore;

use crate::apps::auth::services::oauth::config::{OAuthSettings, ProviderCredentials};
use crate::apps::auth::services::session::RedisUrl;

/// DI-resolvable wrapper around the optional `SocialAuthBackend`.
///
/// Newtype satisfies the DI pseudo-orphan rule
/// (kent8192/reinhardt-web#3468) — `Option<Arc<SocialAuthBackend>>`
/// cannot be registered directly because `SocialAuthBackend` lives in
/// `reinhardt-auth`. Holds `None` when no providers are configured so
/// callers can short-circuit endpoint registration when the feature is
/// effectively disabled.
pub struct OAuthBackendBox(pub Option<Arc<SocialAuthBackend>>);

/// DI factory — singleton scope shares providers and Redis connections.
///
/// Panics on `SocialAuthError` because backend construction failures are
/// deploy-time configuration errors (bad provider config / missing
/// dependencies), not recoverable runtime faults.
#[injectable(scope = "singleton")]
async fn create_oauth_backend(
	#[inject] settings: Depends<OAuthSettings>,
	#[inject] redis_url: Depends<RedisUrl>,
) -> OAuthBackendBox {
	OAuthBackendBox(
		assemble_social_auth_backend(&settings, &redis_url.0)
			.await
			.expect("Failed to construct SocialAuthBackend: check OAuth provider configuration"),
	)
}

pub(in crate::apps::auth) async fn assemble_social_auth_backend(
	settings: &OAuthSettings,
	redis_url: &str,
) -> Result<Option<Arc<SocialAuthBackend>>, SocialAuthError> {
	if settings.enabled_provider_ids().is_empty() {
		return Ok(None);
	}

	let sessions = RedisSessionBackend::new_from_url(redis_url)
		.map_err(|error| SocialAuthError::Storage(error.to_string()))?;
	let mut backend =
		SocialAuthBackend::with_state_store(Arc::new(AsyncSessionStateStore::new(sessions)));

	if let Some(creds) = &settings.github {
		let cfg = github_provider_config(creds);
		let provider = GitHubProvider::new(cfg).await?;
		backend.register_provider(Arc::new(provider));
	}

	Ok(Some(Arc::new(backend)))
}

/// Constructs `ProviderConfig::github(...)` and applies any
/// `REINHARDT_CLOUD_OAUTH_GITHUB_{AUTHORIZE,TOKEN,USERINFO}_URL` env-var
/// overrides on top of it. Tests use these to point the flow at a
/// wiremock-rs server; production leaves them unset and gets the
/// canonical github.com / api.github.com endpoints.
fn github_provider_config(creds: &ProviderCredentials) -> ProviderConfig {
	let mut cfg = ProviderConfig::github(
		creds.client_id.clone(),
		creds.client_secret.clone(),
		creds.redirect_uri.clone(),
	);
	if let Some(oauth2) = cfg.oauth2.as_mut() {
		if let Some(v) = non_empty_env("REINHARDT_CLOUD_OAUTH_GITHUB_AUTHORIZE_URL") {
			oauth2.authorization_endpoint = v;
		}
		if let Some(v) = non_empty_env("REINHARDT_CLOUD_OAUTH_GITHUB_TOKEN_URL") {
			oauth2.token_endpoint = v;
		}
		if let Some(v) = non_empty_env("REINHARDT_CLOUD_OAUTH_GITHUB_USERINFO_URL") {
			oauth2.userinfo_endpoint = Some(v);
		}
	}
	cfg
}

fn non_empty_env(key: &str) -> Option<String> {
	env::var(key).ok().filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::config::test_helpers::{make_test_di_context, set_provider_value};
	use reinhardt::di::Depends;
	use rstest::rstest;
	use serial_test::serial;

	#[rstest]
	#[tokio::test]
	async fn test_oauth_backend_factory_returns_none_when_no_providers_configured() {
		// Arrange — empty OAuthSettings simulates a deployment with no
		// providers enabled. The factory should short-circuit to None
		// rather than constructing an empty backend.
		let ctx = make_test_di_context(|scope| {
			set_provider_value(scope, OAuthSettings::default());
			set_provider_value(scope, RedisUrl("redis://127.0.0.1:6379".into()));
		});

		// Act
		let backend = Depends::<OAuthBackendBox>::builder()
			.resolve(&ctx)
			.await
			.expect("OAuthBackendBox factory should resolve when OAuthSettings is registered");

		// Assert
		assert!(backend.0.is_none());
	}

	#[rstest]
	#[tokio::test]
	#[serial(env_oauth_endpoints)]
	async fn test_oauth_backend_factory_returns_some_when_github_configured() {
		// Arrange — populated OAuthSettings with valid GitHub credentials.
		// Endpoint URLs are unset so the factory uses the canonical GitHub
		// endpoints, which `GitHubProvider::new` accepts without contacting
		// the network.
		let settings = OAuthSettings {
			github: Some(ProviderCredentials {
				client_id: "test-client-id".to_string(),
				client_secret: "test-client-secret".to_string(),
				redirect_uri: "https://example.test/oauth/github/callback".to_string(),
			}),
		};
		let ctx = make_test_di_context(|scope| {
			set_provider_value(scope, settings);
			set_provider_value(scope, RedisUrl("redis://127.0.0.1:6379".into()));
		});

		// Act
		let backend = Depends::<OAuthBackendBox>::builder()
			.resolve(&ctx)
			.await
			.expect("OAuthBackendBox factory should resolve when GitHub credentials are present");

		// Assert
		assert!(
			backend.0.is_some(),
			"backend should be Some when GitHub provider is configured"
		);
	}
}
