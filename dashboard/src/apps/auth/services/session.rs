//! Session management for frontend authentication.

use std::time::Duration;

use reinhardt::RedisSessionBackend;
use reinhardt::middleware::session::{AsyncSessionBackend, SessionData};

use crate::apps::auth::models::User;

/// Create a new session in Redis for the given user.
///
/// Returns the session ID (to be set as cookie value).
pub async fn create_session(user: &User) -> Result<String, String> {
	use reinhardt::BaseUser;

	let backend = get_session_backend()?;
	let mut session = SessionData::new(Duration::from_secs(1800));
	session
		.set("user_id".to_string(), user.id().to_string())
		.map_err(|e| e.to_string())?;
	session
		.set("username".to_string(), user.get_username().to_string())
		.map_err(|e| e.to_string())?;
	session
		.set("is_staff".to_string(), user.is_staff)
		.map_err(|e| e.to_string())?;
	session
		.set("is_superuser".to_string(), user.is_superuser)
		.map_err(|e| e.to_string())?;

	let session_id = session.id.clone();
	backend
		.save(&session)
		.await
		.map_err(|e| format!("Failed to save session: {e}"))?;

	Ok(session_id)
}

/// Destroy a session in Redis.
pub async fn destroy_session(session_id: &str) -> Result<(), String> {
	let backend = get_session_backend()?;
	backend
		.destroy(session_id)
		.await
		.map_err(|e| format!("Failed to destroy session: {e}"))
}

/// Validate a session and return (user_id, username).
/// Used by WebSocket consumer for handshake cookie auth.
pub async fn validate_session(session_id: &str) -> Option<(String, String)> {
	let backend = get_session_backend().ok()?;
	let session = backend.load(session_id).await.ok()??;
	let user_id: String = session.get("user_id")?;
	let username: String = session.get("username")?;
	Some((user_id, username))
}

fn get_session_backend() -> Result<RedisSessionBackend, String> {
	let url = crate::config::settings::get_redis_url().ok_or("Redis URL not configured")?;
	RedisSessionBackend::new_from_url(&url).map_err(|e| format!("Redis connection failed: {e}"))
}
