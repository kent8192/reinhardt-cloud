//! Register view for auth app.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
use reinhardt::http::ViewResult;
use reinhardt::post;
use reinhardt::{BaseUser, Json, Response, StatusCode};
use tracing::error;

use crate::apps::auth::models::User;
use crate::apps::auth::serializers::{RegisterRequest, TokenResponse};
use crate::apps::auth::services;

/// Register new user, persist to database, and return JWT token.
#[post("/auth/register/", name = "auth_register", pre_validate = true)]
pub async fn register(body: Json<RegisterRequest>) -> ViewResult<Response> {
	// Create user with hashed password
	let mut user = User::new(
		body.username.trim().to_string(),
		body.email.trim().to_string(),
		String::new(),
		String::new(),
		None,
		true,
		false,
		false,
	);
	user.set_password(&body.password).map_err(|e| {
		error!("Password hashing failed during registration: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	// Attempt to create — database unique constraint prevents duplicates
	let created = match User::objects().create(&user).await {
		Ok(user) => user,
		Err(e) => {
			// Normalize error message to lowercase for case-insensitive matching.
			// The ORM (reinhardt-db) maps unique constraint violations to
			// `DatabaseError::QueryError(String)` without a structured variant,
			// so string matching is the only detection mechanism available.
			let err_lower = e.to_string().to_lowercase();
			if err_lower.contains("unique") || err_lower.contains("duplicate") {
				// Distinguish which field caused the conflict by checking
				// the PostgreSQL constraint name embedded in the error message.
				let message = if err_lower.contains("auth_user_email_uniq") {
					"Email already exists"
				} else {
					"Username already exists"
				};
				return Err(AppError::Conflict(message.to_string()));
			}
			error!("Failed to create user in database: {e}");
			return Err(AppError::Internal("Internal server error".to_string()));
		}
	};

	let token = services::create_session_token(&created).map_err(|e| {
		error!("JWT token generation failed during registration: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	let resp = TokenResponse::bearer(token);
	Ok(Response::new(StatusCode::CREATED)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
