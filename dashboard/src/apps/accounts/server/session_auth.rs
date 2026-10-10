//! Cookie session authentication, revalidated against the database.
//!
//! On every request this middleware turns the session cookie into the
//! request's authentication state (`AuthState`, which `CurrentUser<User>` and
//! the admin site's Staff gate read). It derives everything from the current
//! `User` row, never from the session record (SR-07):
//!
//! - an unknown, expired, or malformed session is anonymous;
//! - a User that no longer exists, or is deactivated, is anonymous and the
//!   session is destroyed;
//! - Staff status is read fresh, so a grant or revocation applies to the next
//!   request without signing in again.
//!
//! Any failure to decide (Redis or the database unavailable) is anonymous:
//! the request is treated as unauthenticated rather than trusted.

use std::sync::Arc;

use async_trait::async_trait;
use reinhardt::core::exception::Result;
use reinhardt::db::orm::Model;
use reinhardt::http::{AuthState, IsActive, IsAdmin, IsAuthenticated};
use reinhardt::{Handler, Middleware, Request, Response};

use crate::apps::accounts::models::User;
use crate::apps::accounts::server::cookies::{SESSION_COOKIE, request_cookie};
use crate::apps::accounts::services::server::sessions::{SessionService, SessionToken};

/// Authenticates requests from the session cookie.
#[derive(Clone, Debug)]
pub struct SessionAuthMiddleware {
	sessions: SessionService,
}

impl SessionAuthMiddleware {
	/// Create the middleware.
	#[must_use]
	pub fn new(sessions: SessionService) -> Self {
		Self { sessions }
	}

	async fn authenticate(&self, request: &Request) -> AuthState {
		let Some(cookie) = request_cookie(request, SESSION_COOKIE) else {
			return AuthState::anonymous();
		};
		let token = SessionToken::from_cookie(&cookie);
		let user_id = match self.sessions.resolve(&token).await {
			Ok(Some(user_id)) => user_id,
			Ok(None) => return AuthState::anonymous(),
			Err(error) => {
				tracing::warn!(%error, "session lookup failed; treating the request as anonymous");
				return AuthState::anonymous();
			}
		};

		match User::objects()
			.filter(User::field_id().eq(user_id))
			.first()
			.await
		{
			Ok(Some(user)) if user.is_active => {
				AuthState::authenticated(user.id.to_string(), user.is_staff, true)
			}
			Ok(_) => {
				// Deleted or deactivated: the session must not outlive the User.
				let _ = self.sessions.destroy(&token).await;
				AuthState::anonymous()
			}
			Err(error) => {
				tracing::warn!(%error, "user lookup failed; treating the request as anonymous");
				AuthState::anonymous()
			}
		}
	}
}

/// Publish `state` where the framework's extractors and guards look for it.
fn publish(request: &Request, state: AuthState) {
	if state.is_authenticated() {
		request.extensions.insert(state.user_id().to_owned());
	} else {
		let _ = request.extensions.remove::<String>();
	}
	request
		.extensions
		.insert(IsAuthenticated(state.is_authenticated()));
	request.extensions.insert(IsAdmin(state.is_admin()));
	request.extensions.insert(IsActive(state.is_active()));
	request.extensions.insert(state);
}

#[async_trait]
impl Middleware for SessionAuthMiddleware {
	async fn process(&self, request: Request, next: Arc<dyn Handler>) -> Result<Response> {
		let state = self.authenticate(&request).await;
		publish(&request, state);
		next.handle(request).await
	}
}
