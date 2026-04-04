//! Custom JWT authentication middleware for Reinhardt Cloud.
//!
//! Validates `Authorization: Bearer <token>` headers and injects
//! `AuthState` into request extensions for downstream `AuthInfo`
//! extraction. Skips authentication for public endpoints (auth routes).

use std::sync::Arc;

use reinhardt::async_trait::async_trait;
use reinhardt::http::AuthState;
use reinhardt::{Handler, JwtAuth, Middleware, Request, Response};

use crate::apps::auth::views::utils::jwt_secret;

/// JWT authentication middleware.
///
/// Extracts and validates Bearer tokens from the `Authorization` header,
/// then stores an `AuthState` in request extensions so that `AuthInfo`
/// can resolve the authenticated user via dependency injection.
pub struct JwtAuthMiddleware;

#[async_trait]
impl Middleware for JwtAuthMiddleware {
	async fn process(
		&self,
		request: Request,
		next: Arc<dyn Handler>,
	) -> reinhardt::core::exception::Result<Response> {
		let is_admin = request.uri.path().starts_with("/admin/");

		// Extract and validate Bearer token
		if let Some(header_value) = request.headers.get("Authorization")
			&& let Ok(header_str) = header_value.to_str()
			&& let Some(token) = header_str.strip_prefix("Bearer ")
		{
			let secret = jwt_secret().map_err(|e| {
				tracing::error!("JWT secret not configured: {e}");
				reinhardt::core::exception::Error::Authentication(
					"Authentication service unavailable".to_string(),
				)
			})?;
			let auth = JwtAuth::new(secret.as_bytes());
			if let Ok(claims) = auth.verify_token(token)
				&& !claims.is_expired()
			{
				let auth_state =
					AuthState::authenticated(&claims.sub, claims.is_staff, claims.is_superuser);
				request.extensions.insert(auth_state);
			} else if !is_admin {
				// Token present but invalid — reject for API routes only.
				// Admin routes handle their own auth errors via server functions.
				return Err(reinhardt::core::exception::Error::Authentication(
					"Invalid or expired authentication token".to_string(),
				));
			}
		} else if !is_admin {
			// No Authorization header — reject for API routes only.
			// Admin routes allow unauthenticated access (login page, static assets).
			return Err(reinhardt::core::exception::Error::Authentication(
				"Authentication credentials were not provided".to_string(),
			));
		}

		next.handle(request).await
	}

	/// Skip middleware for auth endpoints (login/register), public API docs,
	/// public server functions, and admin static assets.
	///
	/// Admin panel routes are NOT skipped — the middleware parses JWT if
	/// present but does not reject unauthenticated requests (admin handles
	/// its own auth via server functions).
	fn should_continue(&self, request: &Request) -> bool {
		let path = request.uri.path();

		// Public server functions that do not require authentication
		const PUBLIC_SERVER_FNS: &[&str] = &[
			"/api/server_fn/login",
			"/api/server_fn/register",
			"/api/server_fn/logout",
			"/api/server_fn/me",
		];

		!path.starts_with("/api/auth/")
			&& path != "/api/openapi.json"
			&& !path.starts_with("/api/docs")
			&& !path.starts_with("/api/redoc")
			&& !PUBLIC_SERVER_FNS.iter().any(|p| path.starts_with(p))
			&& !path.starts_with("/static/admin/")
	}
}
