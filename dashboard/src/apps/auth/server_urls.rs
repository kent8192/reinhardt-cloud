//! Server-side URLs for auth flows that cannot be expressed as server functions.
//!
//! Browser navigation and email-link callbacks use regular server routes.
//! Interactive form submission remains implemented through `server_fn`.

use reinhardt::auth::social::core::SocialAuthError;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::pages::server_fn::ServerFnRequest;
use reinhardt::{BaseUser, CurrentUser, Path, Query, Response, StatusCode, get};
use serde::Deserialize;
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
	Ok(Response::temporary_redirect(auth.authorization_url))
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
	let is_debug = get_settings().core.debug;
	Ok(Response::temporary_redirect("/")
		.append_header("Set-Cookie", &session_cookie_header(&session_id, is_debug)))
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
