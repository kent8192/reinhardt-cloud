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
use crate::apps::auth::services::oauth::linking::{LinkError, link_or_create_user};
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
			match e {
				// Surface the actionable conflict message to the client so
				// the UI can prompt for "sign in with your existing account
				// first and link from there".
				LinkError::EmailConflict { .. } => AppError::Validation(e.to_string()),
				_ => AppError::Internal("account linking failed".to_string()),
			}
		})?;

	let session_id = create_session(&user).await.map_err(|e| {
		error!("session creation after OAuth failed: {e}");
		AppError::Internal("session creation failed".to_string())
	})?;

	let is_debug = crate::config::settings::get_settings().core.debug;
	let secure_flag = if is_debug { "" } else { "; Secure" };
	let cookie = format!(
		"sessionid={session_id}; HttpOnly; SameSite=Lax; Path=/{secure_flag}; Max-Age=86400"
	);

	Ok(Response::new(StatusCode::FOUND)
		.with_header("Location", "/")
		.with_header("Set-Cookie", &cookie))
}

#[cfg(test)]
mod tests {
	//! Inline unit tests for `oauth_callback`'s pre-flight error paths.
	//!
	//! The full callback exercise (state validation, code exchange,
	//! userinfo fetch, linking, session creation) is covered by the e2e
	//! flow in `tests/e2e/test_oauth_github_login.rs`. This module pins
	//! the cheap-to-test short-circuit branches so view-level coverage
	//! stays above 80%:
	//!
	//!   * provider-not-enabled → 404 (`AppError::NotFound`)
	//!   * unknown provider id  → 404 (defense in depth: must not reach
	//!     the framework registry where it would surface as a 500).

	use super::*;
	use reinhardt::Query;
	use rstest::rstest;
	use serial_test::serial;

	const KEY_ID: &str = "REINHARDT_CLOUD_OAUTH_GITHUB_CLIENT_ID";
	const KEY_SECRET: &str = "REINHARDT_CLOUD_OAUTH_GITHUB_CLIENT_SECRET";
	const KEY_REDIRECT: &str = "REINHARDT_CLOUD_OAUTH_GITHUB_REDIRECT_URI";

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

	fn dummy_query() -> OAuthCallbackQuery {
		OAuthCallbackQuery {
			code: "ignored-code".to_string(),
			state: "ignored-state".to_string(),
		}
	}

	#[rstest]
	#[serial(env_oauth)]
	#[tokio::test]
	async fn test_oauth_callback_returns_404_when_provider_not_enabled() {
		// Arrange — credentials are absent so `OAuthSettings::get("github")`
		// returns None and the view short-circuits before touching the
		// backend or the database.
		let _g = EnvGuard::set(vec![(KEY_ID, None), (KEY_SECRET, None), (KEY_REDIRECT, None)]);

		// Act
		let result =
			oauth_callback_original(Path("github".to_string()), Query(dummy_query())).await;

		// Assert
		match result {
			Err(AppError::NotFound(msg)) => {
				assert_eq!(msg, "OAuth provider not enabled: github");
			}
			other => panic!("expected NotFound, got {other:?}"),
		}
	}

	#[rstest]
	#[serial(env_oauth)]
	#[tokio::test]
	async fn test_oauth_callback_returns_404_for_unknown_provider() {
		// Arrange — even with GitHub fully configured, an unknown
		// provider id must 404 (defense in depth: we must not let the
		// framework registry handle it and surface a generic 500).
		let _g = EnvGuard::set(vec![
			(KEY_ID, Some("test-id")),
			(KEY_SECRET, Some("test-secret")),
			(KEY_REDIRECT, Some("https://example.test/cb")),
		]);

		// Act
		let result =
			oauth_callback_original(Path("gitlab".to_string()), Query(dummy_query())).await;

		// Assert
		match result {
			Err(AppError::NotFound(msg)) => {
				assert_eq!(msg, "OAuth provider not enabled: gitlab");
			}
			other => panic!("expected NotFound, got {other:?}"),
		}
	}
}
