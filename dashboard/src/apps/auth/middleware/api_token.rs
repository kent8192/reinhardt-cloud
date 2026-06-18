//! Bearer-token authentication middleware.
//!
//! Verifies an `Authorization: Bearer <opaque-token>` header against the
//! hashed `ApiKey` store and injects `AuthState` into request extensions —
//! the same injection point `CurrentUser<T>` / `AuthUser<T>` read, so both
//! session-cookie and bearer-token callers unify behind one injection point.
//!
//! The middleware is registered after `CookieSessionAuthMiddleware`: a valid
//! bearer token replaces the session-derived state, while a missing or invalid
//! bearer header leaves any existing session state untouched.

use std::sync::Arc;

use reinhardt::async_trait::async_trait;
use reinhardt::http::AuthState;
use reinhardt::{Handler, Middleware, Request, Response};

use crate::apps::auth::services::api_key::{touch_last_used, verify_api_key};

/// Authenticate CLI callers via a long-lived API token.
pub struct ApiTokenAuthMiddleware;

#[async_trait]
impl Middleware for ApiTokenAuthMiddleware {
	async fn process(
		&self,
		request: Request,
		next: Arc<dyn Handler>,
	) -> reinhardt::core::exception::Result<Response> {
		// Only a verified bearer token may replace the current AuthState.
		// Invalid bearer input falls through so it cannot erase a valid session.
		if let Some(token) = bearer_token(&request) {
			let auth_state = resolve_auth_state_for_bearer(&token).await;
			if auth_state.is_authenticated() {
				request.extensions.insert(auth_state);
			}
		}
		next.handle(request).await
	}
}

/// Resolve an `AuthState` for a bearer plaintext token.
///
/// Factored out so the verification logic is testable without a full
/// `Request`. On success it schedules a non-blocking `last_used_at` update
/// to avoid a write-per-request on the hot path.
pub async fn resolve_auth_state_for_bearer(plaintext: &str) -> AuthState {
	match verify_api_key(plaintext).await {
		Some((user, api_key_id)) => {
			let is_admin = user.is_staff || user.is_superuser;
			// Fire-and-forget last_used_at update (best-effort diagnostics).
			tokio::spawn(async move { touch_last_used(api_key_id).await });
			AuthState::authenticated(user.id().to_string(), is_admin, user.is_active)
		}
		None => AuthState::anonymous(),
	}
}

/// Extract a bearer token from the `Authorization` header, if present.
fn bearer_token(request: &Request) -> Option<String> {
	let header = request.headers.get("Authorization")?.to_str().ok()?;
	let value = header.strip_prefix("Bearer ")?.trim();
	(!value.is_empty()).then(|| value.to_string())
}
