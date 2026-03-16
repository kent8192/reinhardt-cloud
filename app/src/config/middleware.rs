//! Custom JWT authentication middleware for nuages.
//!
//! Validates `Authorization: Bearer <token>` headers and injects
//! `AuthState` into request extensions for downstream `CurrentUser`
//! extraction. Skips authentication for public endpoints (auth routes).

use std::sync::Arc;

use reinhardt::async_trait::async_trait;
use reinhardt::http::AuthState;
use reinhardt::{Handler, JwtAuth, Middleware, Request, Response};

use crate::apps::auth::views::jwt_secret;

/// JWT authentication middleware.
///
/// Extracts and validates Bearer tokens from the `Authorization` header,
/// then stores an `AuthState` in request extensions so that `CurrentUser<User>`
/// can resolve the authenticated user via dependency injection.
pub struct JwtAuthMiddleware;

#[async_trait]
impl Middleware for JwtAuthMiddleware {
	async fn process(
		&self,
		request: Request,
		next: Arc<dyn Handler>,
	) -> reinhardt::core::exception::Result<Response> {
		// Extract and validate Bearer token
		let auth_state = if let Some(header_value) = request.headers.get("Authorization")
			&& let Ok(header_str) = header_value.to_str()
			&& let Some(token) = header_str.strip_prefix("Bearer ")
		{
			let auth = JwtAuth::new(jwt_secret().as_bytes());
			if let Ok(claims) = auth.verify_token(token)
				&& !claims.is_expired()
			{
				AuthState::authenticated(&claims.sub, false, true)
			} else {
				AuthState::anonymous()
			}
		} else {
			AuthState::anonymous()
		};

		// Insert individual values for AuthState::from_extensions() compatibility.
		// from_extensions() looks for separate String (user_id) and bool
		// (is_authenticated) entries, not an AuthState object.
		// Workaround: See https://github.com/kent8192/reinhardt-web/issues/2417
		request.extensions.insert(auth_state.user_id().to_string());
		request.extensions.insert(auth_state.is_authenticated());
		request.extensions.insert(auth_state);

		next.handle(request).await
	}

	/// Skip middleware for auth endpoints (login/register).
	fn should_continue(&self, request: &Request) -> bool {
		let path = request.uri.path();
		!path.starts_with("/api/auth/")
	}
}
