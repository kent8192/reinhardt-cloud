//! Login view for auth app.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::post;
use reinhardt::{Json, Response, StatusCode};
use tracing::error;

use crate::apps::auth::serializers::LoginRequest;
use crate::apps::auth::services::session::{SessionService, session_cookie_header};
use crate::shared::AuthResponse;

/// Authenticate user against database and create a session.
#[post("/login/", name = "login", pre_validate = true)]
pub async fn login(
	Json(body): Json<LoginRequest>,
	#[inject] session_service: Depends<SessionService>,
) -> ViewResult<Response> {
	let user =
		crate::apps::auth::services::verify_credentials(&body.username, &body.password).await?;

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
	let cookie = session_cookie_header(&session_id, is_debug);

	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_header("Set-Cookie", &cookie)
		.with_body(json::to_vec(&resp)?))
}
