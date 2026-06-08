//! Credential verification service.
//!
//! No `#[injectable_factory]` conversion (kent8192/reinhardt-cloud#599):
//! this module is a pure ORM helper. It neither reads global settings
//! nor environment variables, so wrapping it in a DI service would add
//! ceremony without removing any global-state coupling. All inputs are
//! function parameters, and the only collaborator (`User::objects()`)
//! is itself a framework-managed ORM entry point already.

use reinhardt::BaseUser;
use reinhardt::core::exception::Error as AppError;
use reinhardt::db::orm::Model;
use tracing::error;

use crate::apps::auth::models::User;

/// Verify user credentials against the database.
///
/// Returns the authenticated `User` on success, or an `AppError`
/// on failure (invalid credentials, inactive account, or DB error).
pub async fn verify_credentials(username: &str, password: &str) -> Result<User, AppError> {
	let user = User::objects()
		.filter(User::field_username().eq(username.trim().to_string()))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to query user during login: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::Authentication("Invalid credentials".to_string()))?;

	let valid = user.check_password(password).map_err(|e| {
		error!("Password verification failed during login: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;
	if !valid {
		return Err(AppError::Authentication("Invalid credentials".to_string()));
	}

	if !user.is_active() {
		return Err(AppError::Authorization(
			"Email verification required".to_string(),
		));
	}

	Ok(user)
}
