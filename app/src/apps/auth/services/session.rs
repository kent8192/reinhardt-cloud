//! Session cookie management for frontend authentication.

use reinhardt::JwtAuth;

use crate::apps::auth::models::User;
use crate::apps::auth::views::utils::jwt_secret;
use crate::shared::UserInfo;

/// Cookie name for frontend session.
const SESSION_COOKIE_NAME: &str = "nuages_session";

/// Create a session cookie header value for the given user.
pub fn create_session_cookie(user: &User) -> Result<String, String> {
	let auth = JwtAuth::new(jwt_secret().as_bytes());
	let token = auth
		.generate_token(user.id().to_string(), user.username().to_string())
		.map_err(|e| format!("Failed to generate session token: {e}"))?;

	Ok(format!(
		"{SESSION_COOKIE_NAME}={token}; HttpOnly; SameSite=Lax; Path=/"
	))
}

/// Create a cookie header that clears the session.
pub fn clear_session_cookie() -> String {
	format!("{SESSION_COOKIE_NAME}=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0")
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

	let auth = JwtAuth::new(jwt_secret().as_bytes());
	let claims = auth.verify_token(token).ok()?;
	Some((claims.sub, claims.username))
}

/// Build `UserInfo` from a `User` model instance.
pub fn user_to_info(user: &User) -> UserInfo {
	UserInfo {
		id: user.id().to_string(),
		username: user.username().to_string(),
		email: user.email.clone(),
	}
}
