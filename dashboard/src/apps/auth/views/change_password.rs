//! Change password view for auth app.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
use reinhardt::http::ViewResult;
use reinhardt::post;
use reinhardt::{AuthInfo, BaseUser, Json, Response, StatusCode};
use serde::Serialize;
use tracing::error;
use uuid::Uuid;

use crate::apps::auth::models::User;
use crate::apps::auth::serializers::ChangePasswordRequest;

/// Simple success message response.
#[derive(Debug, Serialize)]
struct MessageResponse {
	message: String,
}

/// Change the authenticated user's password.
///
/// Requires the current (old) password for verification before
/// setting the new password.
#[post(
	"/auth/change-password/",
	name = "auth_change_password",
	pre_validate = true
)]
pub async fn change_password(
	body: Json<ChangePasswordRequest>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;

	let mut user = User::objects()
		.filter(
			User::field_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
		.first()
		.await
		.map_err(|e| {
			error!("Failed to query user for password change: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

	// Verify the old password
	let valid = user.check_password(&body.old_password).map_err(|e| {
		error!("Password verification failed during password change: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;
	if !valid {
		return Err(AppError::Authentication(
			"Current password is incorrect".to_string(),
		));
	}

	// Set the new password (hashes automatically)
	user.set_password(&body.new_password).map_err(|e| {
		error!("Password hashing failed during password change: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	// Persist the updated password hash
	User::objects().update(&user).await.map_err(|e| {
		error!("Failed to update password in database: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	let resp = MessageResponse {
		message: "Password changed successfully".to_string(),
	};
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
