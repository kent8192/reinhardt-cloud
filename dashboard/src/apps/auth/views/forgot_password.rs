//! Forgot-password view.
//!
//! Accepts an email and sends a password reset link. Always returns 200
//! regardless of whether the email exists (prevents user enumeration).

use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::{BaseUser, Json, Response, StatusCode, post};
use tracing::{debug, error};

use crate::apps::auth::models::User;
use crate::apps::auth::serializers::ForgotPasswordRequest;
use crate::apps::auth::services::email::EmailService;
use crate::apps::auth::services::token::{TokenPurpose, generate_token};

/// Request a password reset email.
///
/// `POST /api/auth/forgot-password/`
///
/// Always returns 200 with a generic message to prevent user enumeration.
#[post("/forgot-password/", name = "forgot-password", pre_validate = true)]
pub async fn forgot_password(
	Json(body): Json<ForgotPasswordRequest>,
	#[inject] email_service: Depends<EmailService>,
) -> ViewResult<Response> {
	let settings = crate::config::settings::get_settings();
	let secret_key = settings.core.secret_key.clone();

	// Look up user by email — but always return 200 either way
	let email = body.email.trim().to_lowercase();
	let user_result = User::objects()
		.filter(User::field_email().eq(email.clone()))
		.first()
		.await;

	match user_result {
		Ok(Some(user)) if user.is_active() => {
			let password_hash = user.password_hash.as_deref().unwrap_or("");
			let token = generate_token(
				TokenPurpose::PasswordReset,
				&user.id,
				password_hash,
				&secret_key,
			);

			// Build reset URL using the frontend base URL
			let port = std::env::var("PORT").unwrap_or_else(|_| "8000".to_string());
			let base_url = std::env::var("REINHARDT_CLOUD_BASE_URL")
				.unwrap_or_else(|_| format!("http://localhost:{port}"));
			let reset_url = format!("{base_url}/api/auth/reset-password/{token}/");

			// Log and fall through on send error to preserve anti-enumeration
			// guarantee (returning 500 only for active users leaks user existence).
			if let Err(e) = email_service
				.send_password_reset_email(&email, &reset_url)
				.await
			{
				error!("Failed to send password reset email: {e}");
			} else {
				debug!("Password reset email sent");
			}
		}
		Ok(Some(_)) => {
			debug!("Password reset requested for inactive account");
		}
		Ok(None) => {
			debug!("Password reset requested for non-existent account");
		}
		Err(e) => {
			error!("Database error during password reset lookup: {e}");
		}
	}

	// Always return 200 to prevent user enumeration
	let body = serde_json::json!({
		"success": true,
		"message": "If an account with this email exists, a password reset link has been sent"
	});
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&body)?))
}
