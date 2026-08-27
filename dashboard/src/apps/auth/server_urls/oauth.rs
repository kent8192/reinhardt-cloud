//! OAuth browser navigation and callback routes.
//!
//! Browser navigation and email-link callbacks use regular server routes.
//! Interactive form submission remains implemented through `server_fn`.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use reinhardt::auth::social::backend::SocialAuthBackend;
use reinhardt::auth::social::core::SocialAuthError;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
use reinhardt::di::Depends;
use reinhardt::di::params::{CookieName, CookieNamed, SessionId};
use reinhardt::http::ViewResult;
use reinhardt::{Path, Query, Response, get};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use tracing::error;
use uuid::Uuid;

use crate::apps::auth::models::User;
use crate::apps::auth::services::oauth::OAuthBackendBox;
use crate::apps::auth::services::oauth::linking::{link_or_create_user, link_user_to_provider};
use crate::apps::auth::services::oauth::storage::OrmSocialAccountStorage;
use crate::apps::auth::services::session::{SessionService, session_cookie_header};
use crate::config::settings::get_settings;

type HmacSha256 = Hmac<sha2::Sha256>;

// Workaround for kent8192/reinhardt-web#6197 (tracked in reinhardt-cloud#895).
// Remove when async session-backed OAuth state carries browser-bound context.
//
// Ideal implementation (without workaround):
//   backend.begin_auth_with_context(provider_id, session_binding, link_intent).await
//   backend.handle_callback_with_context(provider_id, code, state, session_binding).await
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

#[derive(Debug, Deserialize, Serialize)]
struct OAuthLinkIntent {
	provider_id: String,
	state: String,
	user_id: Uuid,
	session_binding: String,
	expires_at: i64,
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

fn oauth_link_intent_signature(payload: &str, secret_key: &str) -> String {
	let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes())
		.expect("HMAC accepts secret keys of any size");
	mac.update(b"reinhardt-cloud-oauth-link-intent-v1");
	mac.update(payload.as_bytes());
	URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

fn oauth_link_session_binding(session_id: &str, secret_key: &str) -> String {
	let mut mac = HmacSha256::new_from_slice(secret_key.as_bytes())
		.expect("HMAC accepts secret keys of any size");
	mac.update(b"reinhardt-cloud-oauth-link-session-binding-v1");
	mac.update(session_id.as_bytes());
	URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

pub(in crate::apps::auth) fn oauth_link_intent_value(
	provider_id: &str,
	state: &str,
	user_id: Uuid,
	session_id: &str,
	expires_at: i64,
	secret_key: &str,
) -> Result<String, AppError> {
	let intent = OAuthLinkIntent {
		provider_id: provider_id.to_string(),
		state: state.to_string(),
		user_id,
		session_binding: oauth_link_session_binding(session_id, secret_key),
		expires_at,
	};
	let payload = json::to_vec(&intent).map_err(|_| {
		AppError::Internal("Failed to create OAuth account-link intent".to_string())
	})?;
	let payload = URL_SAFE_NO_PAD.encode(payload);
	let signature = oauth_link_intent_signature(&payload, secret_key);
	Ok(format!("{OAUTH_LINK_INTENT_PREFIX}{payload}.{signature}"))
}

pub(in crate::apps::auth) fn oauth_link_intent_cookie_header(
	provider_id: &str,
	state: &str,
	user_id: Uuid,
	session_id: &str,
	secret_key: &str,
	debug: bool,
) -> Result<String, AppError> {
	let expires_at = chrono::Utc::now().timestamp() + OAUTH_STATE_COOKIE_MAX_AGE_SECONDS as i64;
	let value = oauth_link_intent_value(
		provider_id,
		state,
		user_id,
		session_id,
		expires_at,
		secret_key,
	)?;
	let secure_flag = if debug { "" } else { "; Secure" };
	Ok(format!(
		"{OAUTH_STATE_COOKIE_NAME}={value}; HttpOnly; SameSite=Lax; Path=/api/auth/oauth/{provider_id}/callback/{secure_flag}; Max-Age={OAUTH_STATE_COOKIE_MAX_AGE_SECONDS}"
	))
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

pub(in crate::apps::auth) fn validate_oauth_link_intent_value(
	value: &str,
	provider_id: &str,
	state: &str,
	session_id: &str,
	secret_key: &str,
	now: i64,
) -> Result<Uuid, AppError> {
	let invalid_intent =
		|| AppError::Validation("OAuth account-link intent is invalid or expired".to_string());
	let value = value
		.strip_prefix(OAUTH_LINK_INTENT_PREFIX)
		.ok_or_else(invalid_intent)?;
	let (payload, signature) = value.split_once('.').ok_or_else(invalid_intent)?;
	let expected_signature = oauth_link_intent_signature(payload, secret_key);
	if signature
		.as_bytes()
		.ct_eq(expected_signature.as_bytes())
		.unwrap_u8()
		!= 1
	{
		return Err(invalid_intent());
	}
	let payload = URL_SAFE_NO_PAD
		.decode(payload)
		.map_err(|_| invalid_intent())?;
	let intent: OAuthLinkIntent = json::from_slice(&payload).map_err(|_| invalid_intent())?;
	if intent.expires_at <= now || intent.provider_id != provider_id || intent.state != state {
		return Err(invalid_intent());
	}
	let expected_session_binding = oauth_link_session_binding(session_id, secret_key);
	if intent
		.session_binding
		.as_bytes()
		.ct_eq(expected_session_binding.as_bytes())
		.unwrap_u8()
		!= 1
	{
		return Err(invalid_intent());
	}
	Ok(intent.user_id)
}

async fn validate_oauth_state_cookie(
	cookie_signature: Option<&str>,
	session_id: Option<&str>,
	provider_id: &str,
	state: &str,
	secret_key: &str,
	session_service: &SessionService,
) -> Result<Option<Uuid>, AppError> {
	let Some(cookie_signature) = cookie_signature else {
		return Err(AppError::Validation(
			"OAuth state cookie is missing or expired".to_string(),
		));
	};
	if cookie_signature.starts_with(OAUTH_LINK_INTENT_PREFIX) {
		let account_link_session =
			current_user_for_account_link_intent(session_id, session_service)
				.await?
				.ok_or_else(|| {
					AppError::Authentication(
						"OAuth account-link session is missing, inactive, or expired".to_string(),
					)
				})?;
		let intent_user_id = validate_oauth_link_intent_value(
			cookie_signature,
			provider_id,
			state,
			&account_link_session.session_id,
			secret_key,
			chrono::Utc::now().timestamp(),
		)?;
		if account_link_session.user.id != intent_user_id {
			return Err(AppError::Authentication(
				"OAuth account-link session no longer matches its intent".to_string(),
			));
		}
		return Ok(Some(intent_user_id));
	}
	let expected_signature = oauth_state_cookie_signature(provider_id, state, secret_key);
	if cookie_signature
		.as_bytes()
		.ct_eq(expected_signature.as_bytes())
		.unwrap_u8()
		!= 1
	{
		return Err(AppError::Validation("OAuth state mismatch".to_string()));
	}
	Ok(None)
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
	let auth = backend
		.begin_auth(&provider_id, None, None)
		.await
		.map_err(map_oauth_error)?;
	let settings = get_settings();
	let state_cookie = match account_link_session {
		Some(account_link_session) => oauth_link_intent_cookie_header(
			&provider_id,
			&auth.state,
			account_link_session.user.id,
			&account_link_session.session_id,
			&settings.core.secret_key,
			settings.core.debug,
		)?,
		None => oauth_state_cookie_header(
			&provider_id,
			&auth.state,
			&settings.core.secret_key,
			settings.core.debug,
		),
	};
	Ok(Response::temporary_redirect(auth.authorization_url)
		.append_header("Set-Cookie", &state_cookie))
}

/// Complete an OAuth authorization flow and establish a dashboard session.
///
/// A signed short-lived account-link intent is captured at the start route
/// and validated before this callback exchanges provider credentials. The
/// callback derives link ownership only from its signed intent. It also
/// validates that the ambient `sessionid` cookie still matches the intent's
/// session binding, because browsers send `SameSite=Lax` cookies on top-level
/// callback navigations and a logout or session swap must invalidate the flow.
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
	let account_link_user_id = validate_oauth_state_cookie(
		oauth_state.as_deref(),
		session_id.as_deref(),
		&provider_id,
		&query.state,
		&settings.core.secret_key,
		&session_service,
	)
	.await?;
	let backend = oauth_backend(&backend, &provider_id)?;
	let result = backend
		.handle_callback(&provider_id, &query.code, &query.state)
		.await
		.map_err(map_oauth_error)?;
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
