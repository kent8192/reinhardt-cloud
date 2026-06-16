//! Construction of the framework `SocialAuthBackend`.
//!
//! Wires together:
//!   * the configured providers (today: `GitHubProvider`, see #428); and
//!   * an `InMemoryStateStore` for OAuth `state` and PKCE verifiers.
//!
//! Exposes [`OAuthBackendBox`] (newtype around `Option<Arc<SocialAuthBackend>>`)
//! resolved via `#[injectable]`.
//!
//! ## State-store choice
//!
//! The framework's only `SessionBackend`-backed state store
//! (`SessionStateStore`) requires a *synchronous* `SessionBackend`, but
//! the dashboard's `RedisSessionBackend` only implements
//! `AsyncSessionBackend`. There is no Redis-or-async `StateStore` impl
//! upstream as of reinhardt-web `main`, so we use
//! `InMemoryStateStore`. Implication: the dashboard is single-instance
//! for OAuth flow purposes — a user who starts the flow on one pod and
//! is routed to another for the callback will see `InvalidState`. State
//! is held for ~10 minutes, so the only steady-state symptom is the
//! occasional retry; the flow itself remains secure (state + PKCE are
//! still per-request and CSRF-bound).
//!
//! Tracked for follow-up: this should move to a Redis-backed StateStore
//! once upstream `reinhardt-auth` exposes one. (Unrelated to the now-merged
//! `kent8192/reinhardt-web#3986`, which added `GenericOidcProvider` but
//! left the state-store backend story unchanged.)
//!
//! ## State-store sharing across requests
//!
//! `begin_auth` (in the `start` view) and `handle_callback` (in the
//! `callback` view) MUST observe the same `StateStore` instance,
//! otherwise the state created on `/start/` is invisible to `/callback/`
//! and every flow returns `InvalidState`. The DI singleton scope on
//! [`OAuthBackendBox`] guarantees a single backend instance (and
//! therefore a single state store) across the lifetime of the process.
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

use reinhardt::auth::social::backend::SocialAuthBackend;
use reinhardt::auth::social::core::config::ProviderConfig;
use reinhardt::auth::social::core::error::SocialAuthError;
use reinhardt::auth::social::flow::state::InMemoryStateStore;
use reinhardt::auth::social::providers::github::GitHubProvider;
use reinhardt::di::{Depends, FactoryOutput};

use crate::apps::auth::services::oauth::config::{
	OAuthSettings, OAuthSettingsKey, ProviderCredentials,
};

/// DI-resolvable wrapper around the optional `SocialAuthBackend`.
///
/// Newtype satisfies the DI pseudo-orphan rule
/// (kent8192/reinhardt-web#3468) — `Option<Arc<SocialAuthBackend>>`
/// cannot be registered directly because `SocialAuthBackend` lives in
/// `reinhardt-auth`. Holds `None` when no providers are configured so
/// callers can short-circuit endpoint registration when the feature is
/// effectively disabled.
pub struct OAuthBackendBox(pub Option<Arc<SocialAuthBackend>>);

#[reinhardt::di::injectable_key]
pub struct OAuthBackendBoxKey;

/// DI factory — singleton scope so the state store and registered
/// providers are shared across all requests for the lifetime of the
/// process. Replaces the previous process-wide `OnceLock<Arc<InMemoryStateStore>>`
/// (kent8192/reinhardt-cloud#599 β2 decision: rely on SingletonScope
/// for single-instance semantics rather than a hand-rolled OnceLock).
///
/// Panics on `SocialAuthError` because backend construction failures are
/// deploy-time configuration errors (bad provider config / missing
/// dependencies), not recoverable runtime faults.
#[reinhardt::di::injectable(scope = "singleton")]
async fn create_oauth_backend(
	#[inject] settings: Depends<OAuthSettingsKey, OAuthSettings>,
) -> FactoryOutput<OAuthBackendBoxKey, OAuthBackendBox> {
	FactoryOutput::new(OAuthBackendBox(
		assemble_social_auth_backend(&settings)
			.await
			.expect("Failed to construct SocialAuthBackend: check OAuth provider configuration"),
	))
}

async fn assemble_social_auth_backend(
	settings: &OAuthSettings,
) -> Result<Option<Arc<SocialAuthBackend>>, SocialAuthError> {
	if settings.enabled_provider_ids().is_empty() {
		return Ok(None);
	}

	let mut backend = SocialAuthBackend::with_state_store(Arc::new(InMemoryStateStore::new()));

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
	use crate::config::test_helpers::make_test_di_context;
	use rstest::rstest;

	#[rstest]
	#[tokio::test]
	async fn test_oauth_backend_factory_returns_none_when_no_providers_configured() {
		// Arrange — empty OAuthSettings simulates a deployment with no
		// providers enabled. The factory should short-circuit to None
		// rather than constructing an empty backend.
		let ctx = make_test_di_context(|scope| {
			scope.set(FactoryOutput::<OAuthSettingsKey, OAuthSettings>::new(
				OAuthSettings::default(),
			));
		});

		// Act
		let backend: Arc<FactoryOutput<OAuthBackendBoxKey, OAuthBackendBox>> = ctx
			.resolve::<FactoryOutput<OAuthBackendBoxKey, OAuthBackendBox>>()
			.await
			.expect("OAuthBackendBox factory should resolve when OAuthSettings is registered");

		// Assert
		assert!(backend.0.is_none());
	}

	#[rstest]
	#[tokio::test]
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
			scope.set(FactoryOutput::<OAuthSettingsKey, OAuthSettings>::new(
				settings,
			));
		});

		// Act
		let backend: Arc<FactoryOutput<OAuthBackendBoxKey, OAuthBackendBox>> = ctx
			.resolve::<FactoryOutput<OAuthBackendBoxKey, OAuthBackendBox>>()
			.await
			.expect("OAuthBackendBox factory should resolve when GitHub credentials are present");

		// Assert
		assert!(
			backend.0.is_some(),
			"backend should be Some when GitHub provider is configured"
		);
	}
}
