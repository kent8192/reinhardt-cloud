//! OAuth browser navigation and callback routes.
//!
//! Browser navigation and email-link callbacks use regular server routes.
//! Interactive form submission remains implemented through `server_fn`.

use reinhardt::auth::social::backend::SocialAuthBackend;
use reinhardt::auth::social::core::SocialAuthError;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
use reinhardt::di::Depends;
use reinhardt::di::params::{CookieName, CookieNamed, SessionId};
use reinhardt::http::ViewResult;
use reinhardt::{Path, Query, Response, get};
use serde::Deserialize;
use tracing::error;
use uuid::Uuid;

use crate::apps::auth::models::User;
use crate::apps::auth::services::oauth::OAuthBackendBox;
use crate::apps::auth::services::oauth::linking::{link_or_create_user, link_user_to_provider};
use crate::apps::auth::services::oauth::storage::OrmSocialAccountStorage;
use crate::apps::auth::services::session::{SessionService, session_cookie_header};
use crate::config::settings::get_settings;

pub(in crate::apps::auth) const OAUTH_STATE_COOKIE_NAME: &str = "oauth_state_sig";
const OAUTH_STATE_COOKIE_MAX_AGE_SECONDS: u64 = 600;
const OAUTH_LINK_INTENT_PREFIX: &str = "link.";

pub(in crate::apps::auth) struct OAuthStateCookie;

impl CookieName for OAuthStateCookie {
	const NAME: &'static str = OAUTH_STATE_COOKIE_NAME;
}

#[derive(Debug, Deserialize)]
pub struct OAuthStartQuery {
	intent: Option<String>,
}

impl OAuthStartQuery {
	fn requests_account_link(&self) -> Result<bool, AppError> {
		match self.intent.as_deref() {
			None => Ok(false),
			Some("link") => Ok(true),
			Some(_) => Err(AppError::Validation(
				"Unsupported OAuth flow intent".to_string(),
			)),
		}
	}
}

/// OAuth callback query parameters returned by the provider.
#[derive(Debug, Deserialize)]
pub struct OAuthCallbackQuery {
	code: String,
	state: String,
}

struct AccountLinkSession {
	user: User,
	session_id: String,
}

fn oauth_backend<'a>(
	backend: &'a OAuthBackendBox,
	provider_id: &str,
) -> Result<&'a SocialAuthBackend, AppError> {
	backend
		.0
		.as_deref()
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

pub(in crate::apps::auth) fn oauth_state_cookie_header(
	provider_id: &str,
	binding_nonce: &str,
	debug: bool,
) -> String {
	let secure_flag = if debug { "" } else { "; Secure" };
	format!(
		"{OAUTH_STATE_COOKIE_NAME}={binding_nonce}; HttpOnly; SameSite=Lax; Path=/api/auth/oauth/{provider_id}/callback/{secure_flag}; Max-Age={OAUTH_STATE_COOKIE_MAX_AGE_SECONDS}"
	)
}

/// Encode an unambiguous browser/session binding for the framework state store.
pub(in crate::apps::auth) fn oauth_state_binding(
	binding_nonce: &str,
	session_id: Option<&str>,
) -> Result<Vec<u8>, AppError> {
	if binding_nonce.is_empty() {
		return Err(AppError::Validation(
			"OAuth state cookie is missing or expired".to_string(),
		));
	}
	json::to_vec(&(binding_nonce, session_id))
		.map_err(|_| AppError::Internal("Failed to create OAuth browser binding".to_string()))
}

/// Recover link ownership only from the consumed server-side context.
pub(in crate::apps::auth) fn oauth_account_link_user(
	context: &[u8],
	current_user_id: Option<Uuid>,
) -> Result<Option<Uuid>, AppError> {
	let intended_user: Option<Uuid> = json::from_slice(context)
		.map_err(|_| AppError::Validation("OAuth flow context is invalid".to_string()))?;
	if intended_user != current_user_id {
		return Err(AppError::Authentication(
			"OAuth account-link session no longer matches its intent".to_string(),
		));
	}
	Ok(intended_user)
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

async fn current_user_for_account_link_intent(
	session_id: Option<&str>,
	session_service: &SessionService,
) -> Result<Option<AccountLinkSession>, AppError> {
	let Some(session_id) = session_id else {
		return Ok(None);
	};
	let Some((user_id, _)) = session_service.validate_session(session_id).await else {
		return Ok(None);
	};
	let Ok(user_id) = user_id.parse::<Uuid>() else {
		return Ok(None);
	};
	let user = User::objects()
		.filter(User::field_id().eq(user_id))
		.first()
		.await
		.map_err(|error| {
			error!(?error, "Failed to reload OAuth account-link user");
			AppError::Internal("Internal server error".to_string())
		})?;
	Ok(user
		.filter(|user| user.is_active)
		.map(|user| AccountLinkSession {
			user,
			session_id: session_id.to_string(),
		}))
}

async fn active_user_for_account_link_intent(user_id: Uuid) -> Result<User, AppError> {
	let user = User::objects()
		.filter(User::field_id().eq(user_id))
		.first()
		.await
		.map_err(|error| {
			error!(?error, "Failed to reload OAuth account-link callback user");
			AppError::Internal("Internal server error".to_string())
		})?;
	match user {
		Some(user) if user.is_active => Ok(user),
		Some(_) | None => Err(AppError::Authentication(
			"OAuth account-link intent is no longer valid".to_string(),
		)),
	}
}

/// Start an OAuth authorization flow for a configured provider.
///
/// `GET /api/auth/oauth/{provider_id}/start/`
#[get("/oauth/{provider_id}/start/", name = "oauth-start")]
pub async fn oauth_start(
	Path(provider_id): Path<String>,
	Query(query): Query<OAuthStartQuery>,
	session_id: CookieNamed<SessionId, Option<String>>,
	#[inject] backend: Depends<OAuthBackendBox>,
	#[inject] session_service: Depends<SessionService>,
) -> ViewResult<Response> {
	let account_link_session = if query.requests_account_link()? {
		Some(
			current_user_for_account_link_intent(session_id.as_deref(), &session_service)
				.await?
				.ok_or_else(|| {
					AppError::Authentication("Sign in before linking an OAuth account".to_string())
				})?,
		)
	} else {
		None
	};
	let backend = oauth_backend(&backend, &provider_id)?;
	let binding_nonce = if account_link_session.is_some() {
		format!("{OAUTH_LINK_INTENT_PREFIX}{}", Uuid::new_v4())
	} else {
		Uuid::new_v4().to_string()
	};
	let binding = oauth_state_binding(
		&binding_nonce,
		account_link_session
			.as_ref()
			.map(|session| session.session_id.as_str()),
	)?;
	let context = json::to_vec(&account_link_session.as_ref().map(|session| session.user.id))
		.map_err(|_| AppError::Internal("Failed to create OAuth flow context".to_string()))?;
	let auth = backend
		.begin_auth_with_context(&provider_id, None, None, &binding, context)
		.await
		.map_err(map_oauth_error)?;
	let settings = get_settings();
	let state_cookie = oauth_state_cookie_header(&provider_id, &binding_nonce, settings.core.debug);
	Ok(Response::temporary_redirect(auth.authorization_url)
		.append_header("Set-Cookie", &state_cookie))
}

/// Complete an OAuth authorization flow and establish a dashboard session.
///
/// The framework atomically consumes Redis state and verifies the initiating
/// browser/session binding before exchanging provider credentials. Account-link
/// ownership comes only from that server-side context. Link callbacks also
/// require a live matching session and active user, so logout and session swaps
/// invalidate the flow before provider credentials are exchanged.
///
/// `GET /api/auth/oauth/{provider_id}/callback/`
#[get("/oauth/{provider_id}/callback/", name = "oauth-callback")]
pub async fn oauth_callback(
	Path(provider_id): Path<String>,
	Query(query): Query<OAuthCallbackQuery>,
	oauth_state: CookieNamed<OAuthStateCookie, Option<String>>,
	session_id: CookieNamed<SessionId, Option<String>>,
	#[inject] backend: Depends<OAuthBackendBox>,
	#[inject] session_service: Depends<SessionService>,
) -> ViewResult<Response> {
	let settings = get_settings();
	let binding_nonce = oauth_state.as_deref().ok_or_else(|| {
		AppError::Validation("OAuth state cookie is missing or expired".to_string())
	})?;
	let account_link_session = if binding_nonce.starts_with(OAUTH_LINK_INTENT_PREFIX) {
		Some(
			current_user_for_account_link_intent(session_id.as_deref(), &session_service)
				.await?
				.ok_or_else(|| {
					AppError::Authentication(
						"OAuth account-link session is missing, inactive, or expired".to_string(),
					)
				})?,
		)
	} else {
		None
	};
	let binding = oauth_state_binding(
		binding_nonce,
		account_link_session
			.as_ref()
			.map(|session| session.session_id.as_str()),
	)?;
	let backend = oauth_backend(&backend, &provider_id)?;
	let result = backend
		.handle_callback_with_context(&provider_id, &query.code, &query.state, &binding)
		.await
		.map_err(map_oauth_error)?;
	let account_link_user_id = oauth_account_link_user(
		&result.context,
		account_link_session.as_ref().map(|session| session.user.id),
	)?;
	let result = result.callback;
	let claims = result.claims.ok_or_else(|| {
		AppError::Validation("OAuth provider did not return user claims".to_string())
	})?;
	let storage = OrmSocialAccountStorage::new();
	let user = match account_link_user_id {
		Some(user_id) => {
			let intent_user = active_user_for_account_link_intent(user_id).await?;
			link_user_to_provider(&storage, &provider_id, &claims, intent_user)
				.await
				.map_err(|err| AppError::Validation(err.to_string()))?
		}
		None => link_or_create_user(&storage, &provider_id, &claims, None)
			.await
			.map_err(|err| AppError::Validation(err.to_string()))?,
	};
	let oauth_token = result.token_response.to_oauth_token();
	storage
		.store_token_for_user(user.id, &provider_id, &claims.sub, &oauth_token)
		.await
		.map_err(|err| {
			error!("Failed to persist OAuth token metadata for provider {provider_id}: {err}");
			AppError::Internal("Internal server error".to_string())
		})?;
	let response = Response::temporary_redirect(if account_link_user_id.is_some() {
		"/account"
	} else {
		"/"
	})
	.append_header(
		"Set-Cookie",
		&expired_oauth_state_cookie_header(&provider_id, settings.core.debug),
	);
	if account_link_user_id.is_some() {
		return Ok(response);
	}
	let session_id = session_service
		.create_session(&user)
		.await
		.map_err(map_session_error)?;
	Ok(response.append_header(
		"Set-Cookie",
		&session_cookie_header(&session_id, settings.core.debug),
	))
}
