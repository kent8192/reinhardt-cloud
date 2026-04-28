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

use std::sync::Arc;

use reinhardt_auth::social::backend::SocialAuthBackend;
use reinhardt_auth::social::core::config::ProviderConfig;
use reinhardt_auth::social::core::error::SocialAuthError;
use reinhardt_auth::social::flow::state::InMemoryStateStore;
use reinhardt_auth::social::providers::github::GitHubProvider;

use crate::apps::auth::services::oauth::config::OAuthSettings;

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

	let state_store = Arc::new(InMemoryStateStore::new());
	let mut backend = SocialAuthBackend::with_state_store(state_store);

	if let Some(creds) = &settings.github {
		let cfg = ProviderConfig::github(
			creds.client_id.clone(),
			creds.client_secret.clone(),
			creds.redirect_uri.clone(),
		);
		let provider = GitHubProvider::new(cfg).await?;
		backend.register_provider(Arc::new(provider));
	}

	Ok(Some(Arc::new(backend)))
}
