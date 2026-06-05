//! Register view for auth app.
//!
//! Creates a new user with `is_active = false` and sends a verification
//! email. The user must verify their email before they can log in.

use reinhardt::core::serde::json;
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::post;
use reinhardt::{Json, Response, StatusCode};

use crate::apps::auth::serializers::RegisterRequest;
use crate::apps::auth::services::email::EmailService;
use crate::apps::auth::services::registration::register_inactive_user;
use crate::shared::AuthResponse;

/// Register new user with email verification required.
///
/// Creates the user as inactive (`is_active = false`) and sends a
/// verification email. No session is created until the email is verified.
#[post("/register/", name = "register", pre_validate = true)]
pub async fn register(
	Json(body): Json<RegisterRequest>,
	#[inject] email_service: Depends<EmailService>,
) -> ViewResult<Response> {
	let created = register_inactive_user(
		&body.username,
		&body.email,
		&body.password,
		email_service.as_ref(),
	)
	.await?;

	let resp = AuthResponse {
		success: true,
		user: Some(crate::shared::UserInfo::from(&created)),
	};

	// No session cookie — user must verify email first
	Ok(Response::new(StatusCode::CREATED)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
