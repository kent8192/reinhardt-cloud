//! Profile views for auth app.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Json, Response, StatusCode};
use reinhardt::{get, patch};
use tracing::error;
use uuid::Uuid;

use crate::apps::auth::models::User;
use crate::apps::auth::serializers::{ProfileResponse, UpdateProfileRequest};

/// Return the authenticated user's profile.
#[get("/profile/", name = "auth_profile")]
pub async fn profile(#[inject] AuthInfo(state): AuthInfo) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;

	let user = User::objects()
		.filter(
			User::field_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
		.first()
		.await
		.map_err(|e| {
			error!("Failed to query user profile: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

	let resp = ProfileResponse::from(user);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}

/// Update the authenticated user's profile fields.
///
/// Note: Email changes currently do not require re-authentication or email
/// verification. This should be enhanced with a confirmation flow in a
/// future iteration to prevent unauthorized email takeover.
#[patch("/profile/", name = "auth_profile_update", pre_validate = true)]
pub async fn profile_update(
	body: Json<UpdateProfileRequest>,
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
			error!("Failed to query user for profile update: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

	// Trim values before applying — reject empty-after-trim values to prevent
	// whitespace-only strings from bypassing length(min=1) validation.
	if let Some(ref first_name) = body.first_name {
		let trimmed = first_name.trim();
		if trimmed.is_empty() {
			return Err(AppError::Validation(
				"first_name must not be blank".to_string(),
			));
		}
		user.first_name = trimmed.to_string();
	}
	if let Some(ref last_name) = body.last_name {
		let trimmed = last_name.trim();
		if trimmed.is_empty() {
			return Err(AppError::Validation(
				"last_name must not be blank".to_string(),
			));
		}
		user.last_name = trimmed.to_string();
	}
	if let Some(ref email) = body.email {
		let trimmed = email.trim();
		if trimmed.is_empty() {
			return Err(AppError::Validation("email must not be blank".to_string()));
		}
		user.email = trimmed.to_string();
	}

	let updated = User::objects().update(&user).await.map_err(|e| {
		let err_lower = e.to_string().to_lowercase();
		if err_lower.contains("unique") || err_lower.contains("duplicate") {
			return AppError::Conflict("Email already exists".to_string());
		}
		error!("Failed to update user profile: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	let resp = ProfileResponse::from(updated);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
