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
use reinhardt::http::ViewResult;
use reinhardt::{Path, Response, StatusCode, get};
use tracing::error;

use crate::apps::auth::services::oauth::backend::build_social_auth_backend;
use crate::apps::auth::services::oauth::config::OAuthSettings;

#[get("/oauth/{provider}/start/", name = "oauth_start")]
pub async fn oauth_start(Path(provider): Path<String>) -> ViewResult<Response> {
	let settings = OAuthSettings::from_env();
	if settings.get(&provider).is_none() {
		return Err(AppError::NotFound(format!(
			"OAuth provider not enabled: {provider}"
		)));
	}

	let backend = build_social_auth_backend(&settings)
		.await
		.map_err(|e| {
			error!("OAuth backend init failed: {e}");
			AppError::Internal("OAuth not available".to_string())
		})?
		.ok_or_else(|| AppError::NotFound("OAuth not configured".to_string()))?;

	// `begin_auth` with `None`/`None` lets reinhardt-auth generate a fresh
	// state (cryptographically random) and a PKCE verifier internally. The
	// verifier is persisted into the state store keyed by state and is
	// transparently consumed by `handle_callback`.
	let result = backend.begin_auth(&provider, None, None).await.map_err(|e| {
		error!("begin_auth({provider}) failed: {e}");
		AppError::Internal("OAuth start failed".to_string())
	})?;

	Ok(Response::new(StatusCode::FOUND).with_header("Location", &result.authorization_url))
}
