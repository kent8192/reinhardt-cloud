//! `GET /oauth/{provider}/start/` — kick off the OAuth authorization flow.
//!
//! Builds the provider's authorization URL via `SocialAuthBackend::begin_auth`
//! (which generates a fresh `state`, persists it with PKCE verifier in the
//! state store, and returns the URL to redirect the browser to). Replies
//! with a `302 Found` to that URL.
//!
//! `provider` is rejected as 404 when not enabled in `OAuthSettings`,
//! preventing requests for `/oauth/twitter/start/` etc. from reaching the
//! framework's provider registry where they would surface as a 500.

use reinhardt::core::exception::Error as AppError;
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::{Path, Response, StatusCode, get};
use tracing::error;

use crate::apps::auth::services::oauth::backend::OAuthBackendBox;
use crate::apps::auth::services::oauth::config::OAuthSettings;

#[get("/oauth/{provider}/start/", name = "oauth-start")]
pub async fn oauth_start(
	Path(provider): Path<String>,
	#[inject] settings: Depends<OAuthSettings>,
	#[inject] backend_box: Depends<OAuthBackendBox>,
) -> ViewResult<Response> {
	if settings.get(&provider).is_none() {
		return Err(AppError::NotFound(format!(
			"OAuth provider not enabled: {provider}"
		)));
	}

	let backend = backend_box
		.0
		.as_ref()
		.ok_or_else(|| AppError::NotFound("OAuth not configured".to_string()))?;

	// `begin_auth` with `None`/`None` lets reinhardt-auth generate a fresh
	// state (cryptographically random) and a PKCE verifier internally. The
	// verifier is persisted into the state store keyed by state and is
	// transparently consumed by `handle_callback`.
	let result = backend
		.begin_auth(&provider, None, None)
		.await
		.map_err(|e| {
			error!("begin_auth({provider}) failed: {e}");
			AppError::Internal("OAuth start failed".to_string())
		})?;

	Ok(Response::new(StatusCode::FOUND).with_header("Location", &result.authorization_url))
}

// Inline unit tests for `oauth_start`'s pre-flight error paths
// (provider-not-enabled and unknown-provider) and the redirect happy
// path were removed when the view was migrated to `#[inject]
// Depends<...>` (#599). The same coverage is preserved by:
//   * the OAuthSettings factory test in services/oauth/config.rs,
//   * the OAuthBackendBox factory tests in services/oauth/backend.rs,
//   * and the full e2e start/callback flow in
//     tests/e2e/test_oauth_github_login.rs.
