//! Register view for auth app.
//!
//! Creates a new user with `is_active = false` and sends a verification
//! email. The user must verify their email before they can log in.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
use reinhardt::http::ViewResult;
use reinhardt::post;
use reinhardt::{BaseUser, Json, Response, StatusCode};
use tracing::{error, info};

use crate::apps::auth::models::User;
use crate::apps::auth::serializers::RegisterRequest;
use crate::apps::auth::services::email::{get_email_backend, send_verification_email};
use crate::apps::auth::services::token::{TokenPurpose, generate_token};
use crate::shared::AuthResponse;

/// Register new user with email verification required.
///
/// Creates the user as inactive (`is_active = false`) and sends a
/// verification email. No session is created until the email is verified.
#[post("/register/", name = "auth_register", pre_validate = true)]
pub async fn register(body: Json<RegisterRequest>) -> ViewResult<Response> {
	let settings = crate::config::settings::get_settings();
	let secret_key = settings.core.secret_key.clone();
	let from_email = settings.email.from_email.clone();

	// Create user as inactive — requires email verification to activate
	let mut user = User::new(
		body.username.trim().to_string(),
		body.email.trim().to_string(),
		String::new(),
		String::new(),
		None,
		false,
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

	// Generate verification token and send email
	let token = generate_token(
		TokenPurpose::EmailVerification,
		&created.id,
		"",
		&secret_key,
	);

	let port = std::env::var("PORT").unwrap_or_else(|_| "8000".to_string());
	let base_url = std::env::var("REINHARDT_CLOUD_BASE_URL")
		.unwrap_or_else(|_| format!("http://localhost:{port}"));
	let verification_url = format!("{base_url}/api/auth/verify-email/{token}/");

	let backend = get_email_backend().map_err(|e| {
		error!("Failed to create email backend: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	if let Err(e) = send_verification_email(
		&created.email,
		&created.username,
		&verification_url,
		backend.as_ref(),
		&from_email,
	)
	.await
	{
		error!(
			"Failed to send verification email to {}: {e}",
			created.email
		);
	} else {
		info!("Verification email sent to {}", created.email);
	}

	let resp = AuthResponse {
		success: true,
		user: Some(crate::shared::UserInfo::from(&created)),
	};

	// No session cookie — user must verify email first
	Ok(Response::new(StatusCode::CREATED)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
