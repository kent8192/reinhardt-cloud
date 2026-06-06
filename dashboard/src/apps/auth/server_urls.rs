//! Server-side URLs for auth flows that cannot be expressed as server functions.
//!
//! Browser navigation and email-link callbacks use regular server routes.
//! Interactive form submission remains implemented through `server_fn`.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
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
/// if the user is already active.
#[get("/verify-email/{token}/", name = "verify-email")]
pub async fn verify_email(Path(token): Path<String>) -> ViewResult<Response> {
	let secret_key = crate::config::settings::get_settings()
		.core
		.secret_key
		.clone();

	let user_id = verify_token(&token, TokenPurpose::EmailVerification, "", &secret_key).map_err(
		|e| match e {
			TokenError::Expired => {
				AppError::Validation("Verification link has expired".to_string())
			}
			_ => AppError::Validation("Invalid verification link".to_string()),
		},
	)?;

	let user = User::objects()
		.filter(User::field_id().eq(user_id.to_string()))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to look up user {user_id} for email verification: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::Validation("Invalid verification link".to_string()))?;

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
