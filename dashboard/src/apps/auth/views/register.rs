//! Register view for auth app.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
use reinhardt::http::ViewResult;
use reinhardt::post;
use reinhardt::{BaseUser, Json, Response, StatusCode};
use tracing::error;

use crate::apps::auth::models::User;
use crate::apps::auth::serializers::RegisterRequest;
use crate::apps::auth::services::session::create_session;
use crate::shared::AuthResponse;

/// Register new user, persist to database, and create a session.
#[post("/register/", name = "register", pre_validate = true)]
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

	// Attempt to create -- database unique constraint prevents duplicates
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

	// Create session in Redis
	let session_id = create_session(&created).await.map_err(|e| {
		error!("Session creation failed during registration: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	let resp = AuthResponse {
		success: true,
		user: Some(crate::shared::UserInfo::from(&created)),
	};

	// Set session cookie on the response
	let is_debug = crate::config::settings::get_settings().core.debug;
	let secure_flag = if is_debug { "" } else { "; Secure" };
	let cookie = format!(
		"sessionid={session_id}; HttpOnly; SameSite=Lax; Path=/{secure_flag}; Max-Age=86400"
	);

	Ok(Response::new(StatusCode::CREATED)
		.with_header("Content-Type", "application/json")
		.with_header("Set-Cookie", &cookie)
		.with_body(json::to_vec(&resp)?))
}
