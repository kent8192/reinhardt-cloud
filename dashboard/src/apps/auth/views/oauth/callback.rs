//! `GET /oauth/{provider}/callback/?code=&state=` — finish the OAuth flow.
//!
//! `handle_callback` validates state, exchanges code for an access token,
//! fetches user-info from the provider, and returns `StandardClaims`.
//! Those claims are then resolved into a local `User` by
//! `link_or_create_user` (see `services::oauth::linking`). On success a
//! Reinhardt session is created and a `sessionid` cookie is issued so the
//! freshly-authenticated browser is logged in for subsequent requests.
//!
//! Path (b) "authenticated link" from #428 is intentionally not wired here
//! yet — we always pass `None` for `current_user`, which means an already
//! logged-in user who initiates an OAuth start will end up with a *new*
//! login under the OAuth identity rather than a link onto their existing
//! account. The "logged-in link" UX requires reading the current session
//! cookie inside this view and is left as a follow-up.

use reinhardt::core::exception::Error as AppError;
use reinhardt::http::ViewResult;
use reinhardt::{Path, Query, Response, StatusCode, get};
use serde::Deserialize;
use tracing::error;

use crate::apps::auth::services::oauth::backend::build_social_auth_backend;
use crate::apps::auth::services::oauth::config::OAuthSettings;
use crate::apps::auth::services::oauth::linking::link_or_create_user;
use crate::apps::auth::services::oauth::storage::OrmSocialAccountStorage;
use crate::apps::auth::services::session::create_session;

/// `?code=...&state=...` query parameters as returned by the provider.
#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
	pub code: String,
	pub state: String,
}

#[get("/oauth/{provider}/callback/", name = "oauth_callback")]
pub async fn oauth_callback(
	Path(provider): Path<String>,
	Query(q): Query<OAuthCallbackQuery>,
) -> ViewResult<Response> {
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

	let cb = backend
		.handle_callback(&provider, &q.code, &q.state)
		.await
		.map_err(|e| {
			// Reuse a single generic message so an attacker cannot
			// discriminate "invalid state" from "code exchange failure"
			// from logs / response bodies.
			error!("handle_callback({provider}) failed: {e}");
			AppError::Authentication("OAuth flow failed".to_string())
		})?;

	let claims = cb.claims.ok_or_else(|| {
		error!("provider {provider} returned no user-info claims");
		AppError::Internal("provider returned no user info".to_string())
	})?;

	let storage = OrmSocialAccountStorage::new();
	let user = link_or_create_user(&storage, &provider, &claims, None)
		.await
		.map_err(|e| {
			error!("OAuth linking for {provider} failed: {e}");
			AppError::Internal("account linking failed".to_string())
		})?;

	let session_id = create_session(&user).await.map_err(|e| {
		error!("session creation after OAuth failed: {e}");
		AppError::Internal("session creation failed".to_string())
	})?;

	let is_debug = crate::config::settings::get_settings().core.debug;
	let secure_flag = if is_debug { "" } else { "; Secure" };
	let cookie =
		format!("sessionid={session_id}; HttpOnly; SameSite=Lax; Path=/{secure_flag}; Max-Age=86400");

	Ok(Response::new(StatusCode::FOUND)
		.with_header("Location", "/")
		.with_header("Set-Cookie", &cookie))
}
