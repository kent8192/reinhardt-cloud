//! Construction of the framework `SocialAuthBackend`.
//!
//! Wires together:
//!   * the configured providers (today: `GitHubProvider`, see #428); and
//!   * an `InMemoryStateStore` for OAuth `state` and PKCE verifiers.
//!
//! ## State-store choice
//!
//! The framework's only `SessionBackend`-backed state store
//! (`SessionStateStore`) requires a *synchronous* `SessionBackend`, but
//! the dashboard's `RedisSessionBackend` only implements
//! `AsyncSessionBackend`. There is no Redis-or-async `StateStore` impl
//! upstream as of `reinhardt-web@v0.1.0-rc.22`, so we use
//! `InMemoryStateStore`. Implication: the dashboard is single-instance
//! for OAuth flow purposes — a user who starts the flow on one pod and
//! is routed to another for the callback will see `InvalidState`. State
//! is held for ~10 minutes, so the only steady-state symptom is the
//! occasional retry; the flow itself remains secure (state + PKCE are
//! still per-request and CSRF-bound).
//!
//! Tracked for follow-up: this should move to a Redis-backed StateStore
//! once upstream `reinhardt-auth` exposes one (companion to
//! `kent8192/reinhardt-web#3986`).
//!
//! ## State-store sharing across requests
//!
//! `begin_auth` (in the `start` view) and `handle_callback` (in the
//! `callback` view) MUST observe the same `StateStore` instance,
//! otherwise the state created on `/start/` is invisible to `/callback/`
//! and every flow returns `InvalidState`. Since each view rebuilds the
//! `SocialAuthBackend` per request to pick up env changes, we keep the
//! state store itself in a process-wide `OnceLock` and reuse it on every
//! rebuild. The provider configuration can change between calls (e.g.
//! tests overriding endpoints), but the state store is stable for the
//! lifetime of the process.
//!
//! ## Test-only endpoint overrides
//!
//! When integration tests point the dashboard at a wiremock-rs server,
//! `REINHARDT_CLOUD_OAUTH_GITHUB_AUTHORIZE_URL`,
//! `REINHARDT_CLOUD_OAUTH_GITHUB_TOKEN_URL`, and
//! `REINHARDT_CLOUD_OAUTH_GITHUB_USERINFO_URL` (when set and non-empty)
//! replace the corresponding GitHub URLs in `ProviderConfig::github`'s
//! `OAuth2Config`. This is the only sanctioned way to redirect a flow
//! at a fake provider without forking `reinhardt-auth`.

use std::env;
use std::sync::{Arc, OnceLock};

use reinhardt_auth::social::backend::SocialAuthBackend;
use reinhardt_auth::social::core::config::ProviderConfig;
use reinhardt_auth::social::core::error::SocialAuthError;
use reinhardt_auth::social::flow::state::InMemoryStateStore;
use reinhardt_auth::social::providers::github::GitHubProvider;

use crate::apps::auth::services::oauth::config::{OAuthSettings, ProviderCredentials};

/// Process-wide state store shared across all backend rebuilds. See module
/// documentation for why this is a `OnceLock` rather than per-request.
static STATE_STORE: OnceLock<Arc<InMemoryStateStore>> = OnceLock::new();

fn shared_state_store() -> Arc<InMemoryStateStore> {
	STATE_STORE
		.get_or_init(|| Arc::new(InMemoryStateStore::new()))
		.clone()
}

/// Build a fully wired `SocialAuthBackend` from settings.
///
/// Returns `Ok(None)` if no providers are configured — callers can use this
/// to short-circuit endpoint registration when the feature is effectively
/// disabled.
pub async fn build_social_auth_backend(
	settings: &OAuthSettings,
) -> Result<Option<Arc<SocialAuthBackend>>, SocialAuthError> {
	if settings.enabled_provider_ids().is_empty() {
		return Ok(None);
	}

	let mut backend = SocialAuthBackend::with_state_store(shared_state_store());

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
