//! Reset-password view.
//!
//! Accepts a token from the URL and a new password in the body,
//! then resets the user's password.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
use reinhardt::http::ViewResult;
use reinhardt::{BaseUser, Json, Path, Response, StatusCode, post};
use tracing::{error, info};

use crate::apps::auth::models::User;
use crate::apps::auth::serializers::ResetPasswordRequest;
use crate::apps::auth::services::token::{self, TokenError, TokenPurpose, verify_token};

/// Reset password using a token from the email link.
///
/// `POST /api/auth/reset-password/{token}/`
///
/// The token is in the URL path; the new password is in the request body.
/// The token self-invalidates after use because the password hash changes.
#[post("/reset-password/{token}/", name = "auth_reset_password")]
pub async fn reset_password(
	Path(token_str): Path<String>,
	body: Json<ResetPasswordRequest>,
) -> ViewResult<Response> {
	let secret_key = crate::config::settings::get_settings().core.secret_key.clone();

	// Extract user_id from token payload without full HMAC verification,
	// because we need the user's password_hash for full token verification.
	let user_id = extract_user_id_from_token(&token_str)
		.map_err(|_| AppError::Validation("Invalid or expired reset link".to_string()))?;

	let user = User::objects()
		.filter(
			User::field_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
		.first()
		.await
		.map_err(|e| {
			error!("Failed to look up user {user_id} for password reset: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::Validation("Invalid or expired reset link".to_string()))?;

	// Verify token with actual password hash
	let password_hash = user.password_hash.as_deref().unwrap_or("");
	verify_token(&token_str, TokenPurpose::PasswordReset, password_hash, &secret_key)
		.map_err(|e| match e {
			TokenError::Expired => {
				AppError::Validation("Reset link has expired".to_string())
			}
			_ => AppError::Validation("Invalid or expired reset link".to_string()),
		})?;

	// Set new password
	let mut updated = user;
	updated.set_password(&body.new_password).map_err(|e| {
		error!("Password hashing failed during reset: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	User::objects().update(&updated).await.map_err(|e| {
		error!("Failed to update password for user {user_id}: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	info!("Password reset completed for user {user_id}");

	let resp = serde_json::json!({
		"success": true,
		"message": "Password has been reset successfully"
	});
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}

/// Extract user_id from token payload without full HMAC verification.
///
/// This is needed because we need the user_id to load the user's
/// password_hash, which is required for full token verification.
fn extract_user_id_from_token(token_str: &str) -> Result<uuid::Uuid, ()> {
	let (payload_b64, _sig_b64) = token_str.split_once('.').ok_or(())?;

	// Decode payload
	let payload_bytes = token::base64url_decode(payload_b64)?;
	let payload = String::from_utf8(payload_bytes).map_err(|_| ())?;

	let parts: Vec<&str> = payload.split('|').collect();
	if parts.len() != 4 {
		return Err(());
	}

	parts[1].parse::<uuid::Uuid>().map_err(|_| ())
}
