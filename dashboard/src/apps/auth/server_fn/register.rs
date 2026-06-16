//! Register server function for frontend user creation.
//!
//! Creates a new user with `is_active = false` and sends a verification
//! email. The user must verify their email before they can log in.

use reinhardt::di::Depends;
use reinhardt::pages::server_fn::{ServerFnError, ServerFnRequest, server_fn};

#[cfg(native)]
use reinhardt::core::exception::Error as AppError;

use crate::shared::AuthResponse;

#[cfg(native)]
use crate::apps::auth::services::{EmailService, EmailServiceKey};
#[cfg(native)]
use crate::config::{ProjectSettings, ProjectSettingsKey};

/// Create a new user account with email verification.
///
/// On the server side this creates a new user in the database with a
/// hashed password and `is_active = false`, then sends a verification
/// email. No session cookie is set — the user must verify their email
/// first. Returns an application error if the username or email already exists.
#[server_fn]
pub async fn register(
	username: String,
	email: String,
	password: String,
	#[inject] _http_request: ServerFnRequest,
	#[inject] settings: Depends<ProjectSettingsKey, ProjectSettings>,
	#[inject] email_service: Depends<EmailServiceKey, EmailService>,
) -> Result<AuthResponse, ServerFnError> {
	use crate::apps::auth::services;
	use crate::shared::UserInfo;

	let created = services::register_inactive_user(
		&username,
		&email,
		&password,
		email_service.as_ref(),
		settings.as_ref(),
	)
	.await
	.map_err(server_fn_error_from_app_error)?;

	// No session cookie — user must verify email first
	let user_info = UserInfo::from(&created);
	Ok(AuthResponse {
		success: true,
		user: Some(user_info),
	})
}

#[cfg(native)]
fn server_fn_error_from_app_error(err: AppError) -> ServerFnError {
	match err {
		AppError::Authentication(message)
		| AppError::Conflict(message)
		| AppError::Validation(message)
		| AppError::Http(message) => ServerFnError::application(message),
		AppError::Internal(message) => ServerFnError::application(message),
		_ => ServerFnError::application("Internal server error"),
	}
}
