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
		if let Some(header_value) = request.headers.get("Authorization")
			&& let Ok(header_str) = header_value.to_str()
			&& let Some(token) = header_str.strip_prefix("Bearer ")
		{
			let auth = JwtAuth::new(jwt_secret().as_bytes());
			if let Ok(claims) = auth.verify_token(token)
				&& !claims.is_expired()
			{
				// Inject AuthState for CurrentUser extraction
				let auth_state = AuthState::authenticated(&claims.sub, false, true);
				request.extensions.insert(auth_state);
			}
		}

		next.handle(request).await
	}

	/// Skip middleware for auth endpoints (login/register).
	fn should_continue(&self, request: &Request) -> bool {
		let path = request.uri.path();
		!path.starts_with("/api/auth/")
	}
}
