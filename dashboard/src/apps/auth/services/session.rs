//! Session management for frontend authentication.

use reinhardt::{BaseUser, FullUser, JwtAuth};

use crate::apps::auth::models::User;
use crate::apps::auth::views::utils::jwt_secret;
use crate::shared::UserInfo;

/// Cookie name for frontend session.
const SESSION_COOKIE_NAME: &str = "reinhardt_cloud_session";

/// Generate a raw JWT session token for the given user.
///
/// Returns the token string without cookie formatting. Used by server
/// functions that return the token in the response body.
pub fn create_session_token(user: &User) -> Result<String, String> {
	let auth = JwtAuth::new(jwt_secret().map_err(|e| e.to_string())?.as_bytes());
	auth.generate_token(
		user.id().to_string(),
		user.get_username().to_string(),
		user.is_staff,
		user.is_superuser,
	)
	.map_err(|e| format!("Failed to generate session token: {e}"))
}

/// Session cookie max-age in seconds (24 hours, matches JWT expiry).
const SESSION_MAX_AGE_SECS: u64 = 86400;

/// Create a session cookie header value for the given user.
pub fn create_session_cookie(user: &User) -> Result<String, String> {
	let token = create_session_token(user)?;

	Ok(format!(
		"{SESSION_COOKIE_NAME}={token}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={SESSION_MAX_AGE_SECS}"
	))
}

/// Create a cookie header that clears the session.
pub fn clear_session_cookie() -> String {
	format!("{SESSION_COOKIE_NAME}=; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=0")
}

/// Validate a raw JWT token and return the claims (sub, username).
///
/// Used by server functions that receive the token directly from the
/// WASM client rather than from a cookie header. Rejects expired tokens.
pub fn validate_raw_token(token: &str) -> Option<(String, String)> {
	if token.is_empty() {
		return None;
	}
	let secret = jwt_secret().ok()?;
	let auth = JwtAuth::new(secret.as_bytes());
	let claims = auth.verify_token(token).ok()?;
	Some((claims.sub, claims.username))
}

/// Extract and validate session token from cookie header string.
pub fn validate_session_token(cookie_header: &str) -> Option<(String, String)> {
	let token = cookie_header
		.split(';')
		.filter_map(|s| {
			let s = s.trim();
			s.strip_prefix(&format!("{SESSION_COOKIE_NAME}="))
		})
		.next()?;

	if token.is_empty() {
		return None;
	}

	let secret = jwt_secret().ok()?;
	let auth = JwtAuth::new(secret.as_bytes());
	let claims = auth.verify_token(token).ok()?;
	Some((claims.sub, claims.username))
}

/// Convert a `User` model into a `UserInfo` DTO for frontend consumption.
pub fn user_to_info(user: &User) -> UserInfo {
	UserInfo {
		id: user.id().to_string(),
		username: user.get_username().to_string(),
		email: user.email().to_string(),
	}
}
