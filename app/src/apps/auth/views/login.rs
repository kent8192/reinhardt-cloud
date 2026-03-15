//! Login view for auth app.

use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
use reinhardt::http::ViewResult;
use reinhardt::post;
use reinhardt::{BaseUser, Json, JwtAuth, Response, StatusCode};

use crate::apps::auth::models::User;
use crate::apps::auth::serializers::{LoginRequest, TokenResponse};
use crate::apps::auth::views::jwt_secret;

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
		.map_err(|e| format!("Database error: {e}"))?
		.ok_or_else(|| "Invalid credentials".to_string())?;

	// Verify password
	let valid = user
		.check_password(&body.password)
		.map_err(|e| format!("Password verification failed: {e}"))?;
	if !valid {
		return Err("Invalid credentials".into());
	}

	// Check if user is active
	if !user.is_active() {
		return Err("User account is inactive".into());
	}

	// Generate JWT with UUID as sub claim
	let auth = JwtAuth::new(jwt_secret().as_bytes());
	let token = auth
		.generate_token(user.id().to_string(), user.username().to_string())
		.map_err(|e| format!("Token generation failed: {e}"))?;

	let resp = TokenResponse::bearer(token);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
