//! Profile views for auth app.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Json, Response, StatusCode};
use reinhardt::{get, put};
use tracing::error;
use uuid::Uuid;

use crate::apps::auth::models::User;
use crate::apps::auth::serializers::{ProfileResponse, UpdateProfileRequest};

/// Return the authenticated user's profile.
#[get("/auth/profile/", name = "auth_profile")]
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
#[put("/auth/profile/", name = "auth_profile_update", pre_validate = true)]
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

	// Apply only the fields that were provided
	if let Some(ref first_name) = body.first_name {
		user.first_name = first_name.trim().to_string();
	}
	if let Some(ref last_name) = body.last_name {
		user.last_name = last_name.trim().to_string();
	}
	if let Some(ref email) = body.email {
		user.email = email.trim().to_string();
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
