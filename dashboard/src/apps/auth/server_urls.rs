//! Server-side URLs for auth flows that cannot be expressed as server functions.
//!
//! Browser navigation and email-link callbacks use regular server routes.
//! Interactive form submission remains implemented through `server_fn`.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use reinhardt::auth::social::core::SocialAuthError;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::pages::server_fn::ServerFnRequest;
use reinhardt::{BaseUser, CurrentUser, Path, Query, Response, StatusCode, get};
use serde::Deserialize;
use subtle::ConstantTimeEq;
use tracing::{error, info};

use crate::apps::auth::models::User;
use crate::apps::auth::services::oauth::linking::link_or_create_user;
use crate::apps::auth::services::oauth::storage::OrmSocialAccountStorage;
use crate::apps::auth::services::oauth::{OAuthBackendBox, OAuthBackendBoxKey};
use crate::apps::auth::services::session::{
	SessionService, SessionServiceKey, session_cookie_header, session_id_from_cookie_header,
};
use crate::apps::auth::services::token::{TokenError, TokenPurpose, verify_token};
use crate::config::settings::get_settings;
use crate::config::{ProjectSettings, ProjectSettingsKey};

type HmacSha256 = Hmac<sha2::Sha256>;

pub(in crate::apps::auth) const OAUTH_STATE_COOKIE_NAME: &str = "oauth_state_sig";
const OAUTH_STATE_COOKIE_MAX_AGE_SECONDS: u64 = 600;

/// OAuth callback query parameters returned by the provider.
#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
	code: String,
	state: String,
}

fn oauth_backend(
	backend: &OAuthBackendBox,
	provider_id: &str,
) -> Result<std::sync::Arc<reinhardt::auth::social::backend::SocialAuthBackend>, AppError> {
	backend
		.0
		.clone()
		.filter(|backend| backend.get_provider(provider_id).is_some())
		.ok_or_else(|| AppError::NotFound(format!("OAuth provider not configured: {provider_id}")))
}

fn map_oauth_error(err: SocialAuthError) -> AppError {
	match err {
		SocialAuthError::Provider(_)
		| SocialAuthError::InvalidState
		| SocialAuthError::StateValidation(_)
		| SocialAuthError::PkceValidation(_) => AppError::Validation(err.to_string()),
		_ => AppError::Internal("OAuth authentication failed".to_string()),
	}
}

fn map_session_error(err: impl std::fmt::Display) -> AppError {
	error!("Failed to create OAuth session: {err}");
	AppError::Internal("Internal server error".to_string())
}

pub(in crate::apps::auth) fn cookie_value_from_header(
	cookie_header: &str,
	cookie_name: &str,
) -> Option<String> {
	cookie_header.split(';').find_map(|pair| {
		let pair = pair.trim();
		let (name, value) = pair.split_once('=')?;
		if name.trim() == cookie_name {
			Some(value.trim().to_string())
		} else {
			None
		}
	})
}

pub(in crate::apps::auth) fn oauth_state_cookie_signature(
	provider_id: &str,
	state: &str,
	secret_key: &str,
) -> String {
	let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes())
		.expect("HMAC accepts secret keys of any size");
	mac.update(b"reinhardt-cloud-oauth-state-v1");
	mac.update(provider_id.as_bytes());
	mac.update(b"\0");
	mac.update(state.as_bytes());
	URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

pub(in crate::apps::auth) fn oauth_state_cookie_header(
	provider_id: &str,
	state: &str,
	secret_key: &str,
	debug: bool,
) -> String {
	let secure_flag = if debug { "" } else { "; Secure" };
	let signature = oauth_state_cookie_signature(provider_id, state, secret_key);
	format!(
		"{OAUTH_STATE_COOKIE_NAME}={signature}; HttpOnly; SameSite=Lax; Path=/api/auth/oauth/{provider_id}/callback/{secure_flag}; Max-Age={OAUTH_STATE_COOKIE_MAX_AGE_SECONDS}"
	)
}

pub(in crate::apps::auth) fn expired_oauth_state_cookie_header(
	provider_id: &str,
	debug: bool,
) -> String {
	let secure_flag = if debug { "" } else { "; Secure" };
	format!(
		"{OAUTH_STATE_COOKIE_NAME}=; HttpOnly; SameSite=Lax; Path=/api/auth/oauth/{provider_id}/callback/{secure_flag}; Max-Age=0"
	)
}

fn validate_oauth_state_cookie(
	request: &ServerFnRequest,
	provider_id: &str,
	state: &str,
	secret_key: &str,
) -> Result<(), AppError> {
	let Some(cookie_signature) = request
		.inner()
		.headers
		.get("Cookie")
		.and_then(|v| v.to_str().ok())
		.and_then(|cookie_header| cookie_value_from_header(cookie_header, OAUTH_STATE_COOKIE_NAME))
	else {
		return Err(AppError::Validation(
			"OAuth state cookie is missing or expired".to_string(),
		));
	};
	let expected_signature = oauth_state_cookie_signature(provider_id, state, secret_key);
	if cookie_signature
		.as_bytes()
		.ct_eq(expected_signature.as_bytes())
		.unwrap_u8()
		!= 1
	{
		return Err(AppError::Validation("OAuth state mismatch".to_string()));
	}
	Ok(())
}

async fn current_user_from_cookie(
	request: &ServerFnRequest,
	session_service: &SessionService,
) -> Result<Option<User>, AppError> {
	let Some(session_id) = request
		.inner()
		.headers
		.get("Cookie")
		.and_then(|v| v.to_str().ok())
		.and_then(session_id_from_cookie_header)
	else {
		return Ok(None);
	};
	let Some((user_id, _username)) = session_service.validate_session(&session_id).await else {
		return Ok(None);
	};
	let user = User::objects()
		.filter(User::field_id().eq(user_id.clone()))
		.first()
		.await
		.map_err(|err| {
			error!("Failed to look up session user {user_id} during OAuth callback: {err}");
			AppError::Internal("Internal server error".to_string())
		})?;
	Ok(user)
}

/// Start an OAuth authorization flow for a configured provider.
///
/// `GET /api/auth/oauth/{provider_id}/start/`
#[get("/oauth/{provider_id}/start/", name = "oauth-start")]
pub async fn oauth_start(
	Path(provider_id): Path<String>,
	#[inject] backend: Depends<OAuthBackendBoxKey, OAuthBackendBox>,
) -> ViewResult<Response> {
	let backend = oauth_backend(&backend, &provider_id)?;
	let auth = backend
		.begin_auth(&provider_id, None, None)
		.await
		.map_err(map_oauth_error)?;
	let settings = get_settings();
	Ok(
		Response::temporary_redirect(auth.authorization_url).append_header(
			"Set-Cookie",
			&oauth_state_cookie_header(
				&provider_id,
				&auth.state,
				&settings.core.secret_key,
				settings.core.debug,
			),
		),
	)
}

/// Complete an OAuth authorization flow and establish a dashboard session.
///
/// `GET /api/auth/oauth/{provider_id}/callback/`
#[get("/oauth/{provider_id}/callback/", name = "oauth-callback")]
pub async fn oauth_callback(
	Path(provider_id): Path<String>,
	Query(query): Query<OAuthCallbackQuery>,
	#[inject] http_request: ServerFnRequest,
	#[inject] backend: Depends<OAuthBackendBoxKey, OAuthBackendBox>,
	#[inject] session_service: Depends<SessionServiceKey, SessionService>,
) -> ViewResult<Response> {
	let settings = get_settings();
	validate_oauth_state_cookie(
		&http_request,
		&provider_id,
		&query.state,
		&settings.core.secret_key,
	)?;
	let backend = oauth_backend(&backend, &provider_id)?;
	let result = backend
		.handle_callback(&provider_id, &query.code, &query.state)
		.await
		.map_err(map_oauth_error)?;
	let claims = result.claims.ok_or_else(|| {
		AppError::Validation("OAuth provider did not return user claims".to_string())
	})?;
	let storage = OrmSocialAccountStorage::new();
	let current_user = current_user_from_cookie(&http_request, &session_service).await?;
	let user = link_or_create_user(&storage, &provider_id, &claims, current_user)
		.await
		.map_err(|err| AppError::Validation(err.to_string()))?;
	let oauth_token = result.token_response.to_oauth_token();
	storage
		.store_token_for_user(user.id, &provider_id, &claims.sub, &oauth_token)
		.await
		.map_err(|err| {
			error!("Failed to persist OAuth token metadata for provider {provider_id}: {err}");
			AppError::Internal("Internal server error".to_string())
		})?;
	let session_id = session_service
		.create_session(&user)
		.await
		.map_err(map_session_error)?;
	Ok(Response::temporary_redirect("/")
		.append_header(
			"Set-Cookie",
			&expired_oauth_state_cookie_header(&provider_id, settings.core.debug),
		)
		.append_header(
			"Set-Cookie",
			&session_cookie_header(&session_id, settings.core.debug),
		))
}

/// Verify email address via URL token.
///
/// `GET /api/auth/verify-email/{token}/`
///
/// On success, sets `is_active = true` for the user. Returns 200 even
/// if the user is already active.
#[get("/verify-email/{token}/", name = "verify-email")]
pub async fn verify_email(
	Path(token): Path<String>,
	#[inject] settings: Depends<ProjectSettingsKey, ProjectSettings>,
) -> ViewResult<Response> {
	let user_id = verify_token(
		&token,
		TokenPurpose::EmailVerification,
		"",
		&settings.core.secret_key,
	)
	.map_err(|e| match e {
		TokenError::Expired => AppError::Validation("Verification link has expired".to_string()),
		_ => AppError::Validation("Invalid verification link".to_string()),
	})?;

	let user = User::objects()
		.filter(User::field_id().eq(user_id.to_string()))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to look up user {user_id} for email verification: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::Validation("Invalid verification link".to_string()))?;

	if !user.is_active() {
		let mut updated = user;
		updated.is_active = true;
		User::objects().update(&updated).await.map_err(|e| {
			error!("Failed to activate user {user_id}: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;
		info!("User {user_id} email verified and activated");
	}

	let body = serde_json::json!({
		"success": true,
		"message": "Email verified successfully"
	});
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&body)?))
}

/// CLI token verification + user info.
///
/// `GET /api/auth/me/` — called by `reinhardt-cloud login` to validate the
/// bearer token and resolve the username for local credentials. The
/// `CurrentUser<User>` extractor reads the `AuthState` injected by
/// `ApiTokenAuthMiddleware`; an unauthenticated request yields 401.
#[get("/me/", name = "api-me")]
pub async fn api_me(
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> ViewResult<Response> {
	let body = serde_json::json!({
		"id": user.id,
		"username": user.get_username(),
	});
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&body)?))
}
