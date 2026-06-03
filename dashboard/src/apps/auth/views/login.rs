//! Login view for auth app.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::post;
use reinhardt::{BaseUser, Json, Response, StatusCode};
use tracing::error;

use crate::apps::auth::models::User;
use crate::apps::auth::serializers::LoginRequest;
use crate::apps::auth::services::session::SessionService;
use crate::shared::AuthResponse;

/// Authenticate user against database and create a session.
#[post("/login/", name = "login", pre_validate = true)]
pub async fn login(
	Json(body): Json<LoginRequest>,
	#[inject] session_service: Depends<SessionService>,
) -> ViewResult<Response> {
	// Find user by username
	let user = User::objects()
		.filter(User::field_username().eq(body.username.trim().to_string()))
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

	// Create session in Redis
	let session_id = session_service.create_session(&user).await.map_err(|e| {
		error!("Session creation failed during login: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	let resp = AuthResponse {
		success: true,
		user: Some(crate::shared::UserInfo::from(&user)),
	};

	// Set session cookie on the response
	let is_debug = crate::config::settings::get_settings().core.debug;
	let secure_flag = if is_debug { "" } else { "; Secure" };
	let cookie = format!(
		"sessionid={session_id}; HttpOnly; SameSite=Lax; Path=/{secure_flag}; Max-Age=86400"
	);

	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_header("Set-Cookie", &cookie)
		.with_body(json::to_vec(&resp)?))
}
