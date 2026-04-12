//! Email verification view.
//!
//! Activates a user account when they visit the verification URL from their email.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
use reinhardt::http::ViewResult;
use reinhardt::{BaseUser, Path, Response, StatusCode, get};
use tracing::{error, info};

use crate::apps::auth::models::User;
use crate::apps::auth::services::token::{TokenError, TokenPurpose, verify_token};

/// Verify email address via URL token.
///
/// `GET /api/auth/verify-email/{token}/`
///
/// On success, sets `is_active = true` for the user. Returns 200 even
/// if the user is already active (idempotent).
#[get("/verify-email/{token}/", name = "auth_verify_email")]
pub async fn verify_email(Path(token): Path<String>) -> ViewResult<Response> {
	let secret_key = crate::config::settings::get_settings().core.secret_key.clone();

	let user_id = verify_token(&token, TokenPurpose::EmailVerification, "", &secret_key)
		.map_err(|e| match e {
			TokenError::Expired => {
				AppError::Validation("Verification link has expired".to_string())
			}
			_ => AppError::Validation("Invalid verification link".to_string()),
		})?;

	let user = User::objects()
		.filter(
			User::field_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
		.first()
		.await
		.map_err(|e| {
			error!("Failed to look up user {user_id} for email verification: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::Validation("Invalid verification link".to_string()))?;

	// Idempotent: if already active, just return success
	if !user.is_active() {
		let mut updated = user;
		updated.is_active = true;
		User::objects().update(&updated).await.map_err(|e| {
			error!("Failed to activate user {user_id}: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;
		info!("User {user_id} email verified and activated");
	}

	let body = serde_json::json!({
		"success": true,
		"message": "Email verified successfully"
	});
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&body)?))
}
