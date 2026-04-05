//! Login view for auth app.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
use reinhardt::http::ViewResult;
use reinhardt::post;
use reinhardt::{BaseUser, Json, JwtAuth, Response, StatusCode};
use tracing::error;

use super::utils::jwt_secret;
use crate::apps::auth::models::User;
use crate::apps::auth::serializers::{LoginRequest, TokenResponse};

/// Authenticate user against database and return JWT token.
#[post("/auth/login/", name = "auth_login", pre_validate = true)]
pub async fn login(body: Json<LoginRequest>) -> ViewResult<Response> {
	// Find user by username
	let user = User::objects()
		.filter(
			User::field_username(),
			FilterOperator::Eq,
			FilterValue::String(body.username.trim().to_string()),
		)
		.first()
		.await
		.map_err(|e| {
			error!("Failed to query user during login: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::Authentication("Invalid credentials".to_string()))?;

	// Verify password
	let valid = user.check_password(&body.password).map_err(|e| {
		error!("Password verification failed during login: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;
	if !valid {
		return Err(AppError::Authentication("Invalid credentials".to_string()));
	}

	// Check if user is active (use same generic message to prevent user enumeration)
	if !user.is_active() {
		return Err(AppError::Authentication("Invalid credentials".to_string()));
	}

	// Generate JWT with UUID as sub claim
	let secret = jwt_secret()?;
	let auth = JwtAuth::new(secret.as_bytes());
	let token = auth
		.generate_token(
			user.id().to_string(),
			user.get_username().to_string(),
			user.is_staff,
			user.is_superuser,
		)
		.map_err(|e| {
			error!("JWT token generation failed during login: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;

	let resp = TokenResponse::bearer(token);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
