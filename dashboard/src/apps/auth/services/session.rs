//! Session management for frontend authentication.

use reinhardt::{BaseUser, JwtAuth};

use crate::apps::auth::models::User;
use crate::apps::auth::views::utils::jwt_secret;

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

/// Validate a raw JWT token and return the claims (sub, username).
///
/// Used by the WebSocket consumer to authenticate connections
/// that receive the token directly from the WASM client.
/// Rejects expired tokens.
pub fn validate_raw_token(token: &str) -> Option<(String, String)> {
	if token.is_empty() {
		return None;
	}
	let secret = jwt_secret().ok()?;
	let auth = JwtAuth::new(secret.as_bytes());
	let claims = auth.verify_token(token).ok()?;
	Some((claims.sub, claims.username))
}
