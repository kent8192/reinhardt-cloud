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
#[post("/reset-password/{token}/", name = "reset_password")]
pub async fn reset_password(
	Path(token_str): Path<String>,
	body: Json<ResetPasswordRequest>,
) -> ViewResult<Response> {
	// Manual validation — pre_validate cannot be used because Path<String>
	// does not implement Validate.
	use reinhardt::Validate;
	body.validate()
		.map_err(|e| AppError::Validation(format!("Invalid request: {e}")))?;

	let secret_key = crate::config::settings::get_settings()
		.core
		.secret_key
		.clone();

	// Validate token structure, HMAC signature, purpose, and expiry BEFORE
	// hitting the database. Full verification (password_hash prefix) happens
	// after loading the user, but this rejects malformed/tampered/expired
	// tokens cheaply without a DB query.
	let user_id = validate_token_before_db(&token_str, &secret_key).map_err(|e| match e {
		TokenError::Expired => AppError::Validation("Reset link has expired".to_string()),
		_ => AppError::Validation("Invalid or expired reset link".to_string()),
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
			error!("Failed to look up user {user_id} for password reset: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::Validation("Invalid or expired reset link".to_string()))?;

	// Verify token with actual password hash
	let password_hash = user.password_hash.as_deref().unwrap_or("");
	verify_token(
		&token_str,
		TokenPurpose::PasswordReset,
		password_hash,
		&secret_key,
	)
	.map_err(|e| match e {
		TokenError::Expired => AppError::Validation("Reset link has expired".to_string()),
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

/// Validate token structure, HMAC signature, purpose, and expiry before
/// hitting the database. Returns the embedded user_id on success.
///
/// This does NOT verify the password_hash prefix (that requires the user
/// record), but it rejects invalid/tampered/expired tokens cheaply.
fn validate_token_before_db(token_str: &str, secret_key: &str) -> Result<uuid::Uuid, TokenError> {
	let (payload_b64, sig_b64) = token_str
		.split_once('.')
		.ok_or(TokenError::MalformedToken)?;

	// Verify HMAC signature (constant-time)
	let expected_sig = token::compute_hmac_for_validation(secret_key, payload_b64);
	let provided_sig = token::base64url_decode(sig_b64).map_err(|_| TokenError::MalformedToken)?;
	if subtle::ConstantTimeEq::ct_eq(expected_sig.as_slice(), provided_sig.as_slice()).unwrap_u8()
		!= 1
	{
		return Err(TokenError::InvalidSignature);
	}

	// Decode and parse payload
	let payload_bytes =
		token::base64url_decode(payload_b64).map_err(|_| TokenError::MalformedToken)?;
	let payload = String::from_utf8(payload_bytes).map_err(|_| TokenError::MalformedToken)?;
	let parts: Vec<&str> = payload.split('|').collect();
	if parts.len() != 4 {
		return Err(TokenError::MalformedToken);
	}

	// Verify purpose — reuse TokenPurpose encoding to avoid drift if the
	// discriminator changes.
	if parts[0] != TokenPurpose::PasswordReset.as_str() {
		return Err(TokenError::PurposeMismatch);
	}

	// Parse user_id
	let user_id = parts[1]
		.parse::<uuid::Uuid>()
		.map_err(|_| TokenError::MalformedToken)?;

	// Check expiry
	let expiry_ts = parts[2]
		.parse::<i64>()
		.map_err(|_| TokenError::MalformedToken)?;
	if chrono::Utc::now().timestamp() > expiry_ts {
		return Err(TokenError::Expired);
	}

	Ok(user_id)
}
