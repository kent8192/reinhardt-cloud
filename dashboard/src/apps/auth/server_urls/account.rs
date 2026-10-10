//! Email verification and authenticated account routes.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::{BaseUser, CurrentUser, Path, Response, StatusCode, get};
use tracing::{error, info};

use crate::apps::auth::models::User;
use crate::apps::auth::services::registration::ensure_personal_organization;
use crate::apps::auth::services::token::{TokenError, TokenPurpose, verify_token};
use crate::config::ProjectSettings;

/// Verify an email token and activate its user.
#[get("/verify-email/{token}/", name = "verify-email")]
pub async fn verify_email(
	Path(token): Path<String>,
	#[inject] settings: Depends<ProjectSettings>,
) -> ViewResult<Response> {
	let user_id = verify_token(
		&token,
		TokenPurpose::EmailVerification,
		"",
		&settings.core.secret_key,
	)
	.map_err(|error| match error {
		TokenError::Expired => AppError::Validation("Verification link has expired".to_string()),
		_ => AppError::Validation("Invalid verification link".to_string()),
	})?;

	let user = User::objects()
		.filter(User::field_id().eq(user_id))
		.first()
		.await
		.map_err(|error| {
			error!("Failed to look up user {user_id} for email verification: {error}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::Validation("Invalid verification link".to_string()))?;

	if user.is_active() {
		ensure_personal_organization(&user).await?;
	} else {
		let mut user = user;
		user.is_active = true;
		let user = User::objects().update(&user).await.map_err(|error| {
			error!("Failed to activate user {user_id}: {error}");
			AppError::Internal("Internal server error".to_string())
		})?;
		ensure_personal_organization(&user).await?;
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

/// Return the authenticated user's CLI identity.
#[get("/me/", name = "api-me")]
pub async fn api_me(
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> ViewResult<Response> {
	let body = serde_json::json!({
		"id": user.id,
		"username": user.get_username(),
	});
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&body)?))
}
