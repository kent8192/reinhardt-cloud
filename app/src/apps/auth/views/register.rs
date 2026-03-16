//! Register view for auth app.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
use reinhardt::http::ViewResult;
use reinhardt::post;
use reinhardt::{BaseUser, Json, JwtAuth, Response, StatusCode};

use super::utils::jwt_secret;
use crate::apps::auth::models::User;
use crate::apps::auth::serializers::{RegisterRequest, TokenResponse};

/// Register new user, persist to database, and return JWT token.
#[post("/auth/register/", name = "auth_register", pre_validate = true)]
pub async fn register(body: Json<RegisterRequest>) -> ViewResult<Response> {
	// Check if username already exists
	let existing = User::objects()
		.filter(
			User::field_username(),
			FilterOperator::Eq,
			FilterValue::String(body.username.trim().to_string()),
		)
		.first()
		.await
		.map_err(|e| format!("Database error: {e}"))?;

	if existing.is_some() {
		return Err(AppError::Conflict("Username already exists".to_string()));
	}

	// Create user with hashed password
	let mut user = User::new(
		body.username.trim().to_string(),
		body.email.trim().to_string(),
		None,
		true,
	);
	user.set_password(&body.password)
		.map_err(|e| format!("Password hashing failed: {e}"))?;

	// Persist to database
	let created = User::objects()
		.create(&user)
		.await
		.map_err(|e| format!("Database error: {e}"))?;

	// Generate JWT with UUID as sub claim
	let auth = JwtAuth::new(jwt_secret().as_bytes());
	let token = auth
		.generate_token(created.id().to_string(), created.username().to_string())
		.map_err(|e| format!("Token generation failed: {e}"))?;

	let resp = TokenResponse::bearer(token);
	Ok(Response::new(StatusCode::CREATED)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
