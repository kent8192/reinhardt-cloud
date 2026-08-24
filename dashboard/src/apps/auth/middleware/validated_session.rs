//! Database-backed validation for cookie-session authentication.
//!
//! `CookieSessionAuthMiddleware` establishes that a session is valid, but its
//! serialized identity and privilege fields can outlive the corresponding user
//! record. This middleware runs immediately afterwards and republishes the
//! request authentication state from the current database record.

use std::sync::Arc;

use reinhardt::async_trait::async_trait;
use reinhardt::di::InjectionContext;
use reinhardt::http::{AuthState, IsActive, IsAdmin, IsAuthenticated};
use reinhardt::prelude::{DatabaseConnection, Model};
use reinhardt::{Handler, Middleware, Request, Response};
use uuid::Uuid;

use crate::apps::auth::models::User;

/// Revalidates a cookie-session identity against the current user record.
pub struct ValidatedSessionAuthMiddleware;

impl ValidatedSessionAuthMiddleware {
	async fn validated_auth_state(request: &Request, session_state: AuthState) -> AuthState {
		if !session_state.is_authenticated() {
			return AuthState::anonymous();
		}

		let Ok(user_id) = session_state.user_id().parse::<Uuid>() else {
			return AuthState::anonymous();
		};
		let Some(context) = request.get_di_context::<Arc<InjectionContext>>() else {
			tracing::warn!("Cookie session authentication has no DI context");
			return AuthState::anonymous();
		};
		let Some(db) = context
			.get_singleton::<DatabaseConnection>()
			.or_else(|| context.get_request::<DatabaseConnection>())
		else {
			tracing::warn!("Cookie session authentication has no database connection");
			return AuthState::anonymous();
		};

		let mut db = *db;
		match User::objects().get(user_id).first_with_db(&mut db).await {
			Ok(Some(user)) if user.is_active => AuthState::authenticated(
				user.id.to_string(),
				user.is_staff || user.is_superuser,
				true,
			),
			Ok(Some(_)) | Ok(None) => AuthState::anonymous(),
			Err(error) => {
				tracing::warn!(?error, "Cookie session account validation failed");
				AuthState::anonymous()
			}
		}
	}

	pub(crate) fn publish_auth_state(request: &Request, auth_state: AuthState) {
		if auth_state.is_authenticated() {
			request.extensions.insert(auth_state.user_id().to_owned());
		} else {
			let _ = request.extensions.remove::<String>();
		}
		request
			.extensions
			.insert(IsAuthenticated(auth_state.is_authenticated()));
		request.extensions.insert(IsAdmin(auth_state.is_admin()));
		request.extensions.insert(IsActive(auth_state.is_active()));
		request.extensions.insert(auth_state);
	}
}

#[async_trait]
impl Middleware for ValidatedSessionAuthMiddleware {
	async fn process(
		&self,
		request: Request,
		next: Arc<dyn Handler>,
	) -> reinhardt::core::exception::Result<Response> {
		let Some(session_state) = request.extensions.get::<AuthState>() else {
			return next.handle(request).await;
		};

		let auth_state = Self::validated_auth_state(&request, session_state).await;
		Self::publish_auth_state(&request, auth_state);
		next.handle(request).await
	}
}
