//! Session management for frontend authentication.
//!
//! Provides [`SessionService`] resolved via `#[injectable_factory]`,
//! capturing a Redis-backed [`AsyncSessionBackend`] once at factory
//! time and exposing async session lifecycle methods. The legacy free
//! functions [`create_session`], [`destroy_session`] and
//! [`validate_session`] are retained as thin adapters during the
//! kent8192/reinhardt-cloud#599 caller migration and will be removed
//! once all callers resolve [`SessionService`] via DI.

use std::sync::Arc;
use std::time::Duration;

use reinhardt::RedisSessionBackend;
use reinhardt::di::{Depends, injectable_factory};
use reinhardt::middleware::session::{AsyncSessionBackend, SessionData};

use crate::apps::auth::models::User;

/// Redis connection URL captured at DI resolution time.
///
/// Wrapper newtype to satisfy the DI pseudo-orphan rule
/// (kent8192/reinhardt-web#3468) — `String` cannot be registered
/// directly. Singleton-scoped so the URL is read once and shared.
pub struct RedisUrl(pub String);

/// DI factory — resolves the Redis URL from settings.
#[injectable_factory(scope = "singleton")]
async fn create_redis_url() -> RedisUrl {
	RedisUrl(
		crate::config::settings::get_redis_url()
			.expect("Redis URL not configured: set redis_url in TOML or REDIS_URL env var"),
	)
}

/// Session lifecycle service backed by a Redis [`AsyncSessionBackend`].
///
/// The backend is constructed once at factory time and shared across
/// all requests; individual `create_session` / `destroy_session` /
/// `validate_session` calls reuse the same connection pool instead of
/// re-resolving Redis configuration on every operation.
pub struct SessionService {
	backend: Arc<dyn AsyncSessionBackend>,
}

/// DI factory — `singleton` because the Redis-backed session store is
/// reusable across requests and connection setup is expensive.
#[injectable_factory(scope = "singleton")]
async fn create_session_service(#[inject] redis_url: Depends<RedisUrl>) -> SessionService {
	let backend = RedisSessionBackend::new_from_url(&redis_url.0)
		.expect("Failed to construct Redis session backend");
	SessionService {
		backend: Arc::new(backend),
	}
}

impl SessionService {
	/// Construct a service from an existing backend without going
	/// through the DI factory. Intended for inline view-level unit
	/// tests that need a hand-built service for short-circuit branches
	/// where the backend is never actually invoked.
	pub fn from_backend(backend: Arc<dyn AsyncSessionBackend>) -> Self {
		Self { backend }
	}

	/// Create a new session in Redis for the given user. Returns the
	/// session ID, which the caller sets as a cookie value.
	pub async fn create_session(&self, user: &User) -> Result<String, String> {
		use reinhardt::BaseUser;

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
		self.backend
			.save(&session)
			.await
			.map_err(|e| format!("Failed to save session: {e}"))?;

		Ok(session_id)
	}

	/// Destroy a session in Redis.
	pub async fn destroy_session(&self, session_id: &str) -> Result<(), String> {
		self.backend
			.destroy(session_id)
			.await
			.map_err(|e| format!("Failed to destroy session: {e}"))
	}

	/// Validate a session and return `(user_id, username)`. Used by
	/// the WebSocket consumer for handshake cookie auth.
	pub async fn validate_session(&self, session_id: &str) -> Option<(String, String)> {
		let session = self.backend.load(session_id).await.ok()??;
		let user_id: String = session.get("user_id")?;
		let username: String = session.get("username")?;
		Some((user_id, username))
	}
}

/// Create a new session in Redis for the given user.
///
/// Retained as a thin adapter while callers migrate to resolving
/// [`SessionService`] via DI (kent8192/reinhardt-cloud#599).
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
///
/// Retained as a thin adapter while callers migrate to
/// [`SessionService`].
pub async fn destroy_session(session_id: &str) -> Result<(), String> {
	let backend = get_session_backend()?;
	backend
		.destroy(session_id)
		.await
		.map_err(|e| format!("Failed to destroy session: {e}"))
}

/// Validate a session and return `(user_id, username)`.
///
/// Retained as a thin adapter while callers migrate to
/// [`SessionService`].
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::config::test_helpers::make_test_di_context;
	use rstest::rstest;

	#[rstest]
	#[tokio::test]
	async fn test_session_service_factory_resolves_with_overridden_redis_url() {
		// Arrange — override RedisUrl so the factory wires the
		// hand-built dependency. `RedisSessionBackend::new_from_url`
		// only parses the URL during construction; it does not
		// require a live Redis until an actual session operation runs.
		let ctx = make_test_di_context(|scope| {
			scope.set(RedisUrl("redis://127.0.0.1:6379".into()));
		});

		// Act
		let svc: Arc<SessionService> = ctx
			.resolve::<SessionService>()
			.await
			.expect("SessionService factory should resolve when RedisUrl is registered");

		// Assert — factory wired the backend; service is usable.
		// We do not exercise the backend here because that would
		// require a live Redis instance; integration tests cover
		// the round-trip path against a real Redis container.
		let _ = svc;
	}
}
