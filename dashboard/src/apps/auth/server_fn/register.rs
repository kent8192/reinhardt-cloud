//! Register server function for frontend user creation.
//!
//! Creates a new user with `is_active = false` and sends a verification
//! email. The user must verify their email before they can log in.

#[cfg(native)]
use reinhardt::di::Depends;
#[cfg(native)]
use reinhardt::pages::server_fn::ServerFnRequest;
use reinhardt::pages::server_fn::{ServerFnError, server_fn};

#[cfg(native)]
use reinhardt::core::exception::Error as AppError;

use crate::apps::auth::serializers::RegisterRequest;
#[cfg(native)]
use crate::apps::auth::services::EmailService;
#[cfg(native)]
use crate::config::ProjectSettings;
use crate::shared::AuthResponse;

/// Create a new user account with email verification.
///
/// On the server side this creates a new user in the database with a
/// hashed password and `is_active = false`, then sends a verification
/// email. No session cookie is set — the user must verify their email
/// first. Returns an application error if the username or email already exists.
#[server_fn(pre_validate = true)]
pub async fn register(
	request: RegisterRequest,
	#[inject] _http_request: ServerFnRequest,
	#[inject] settings: Depends<ProjectSettings>,
	#[inject] email_service: Depends<EmailService>,
) -> Result<AuthResponse, ServerFnError> {
	use crate::apps::auth::services;
	use crate::shared::UserInfo;

	let created = services::register_inactive_user(
		&request.username,
		&request.email,
		&request.password,
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

#[cfg(all(test, native))]
mod tests {
	use std::collections::BTreeSet;

	use reinhardt::pages::server_fn::ServerFnErrorKind;
	use rstest::rstest;

	use super::*;

	#[rstest]
	fn register_request_validation_preserves_structured_field_errors() {
		// Arrange
		let request = RegisterRequest {
			username: "ab".to_string(),
			email: "invalid".to_string(),
			password: "short".to_string(),
		};

		// Act
		let error = reinhardt::Validate::validate(&request)
			.map_err(ServerFnError::from)
			.expect_err("invalid registration request should not reach registration service");

		// Assert
		assert_eq!(error.kind(), ServerFnErrorKind::Validation);
		assert_eq!(
			error
				.field_errors()
				.iter()
				.map(|field| field.field())
				.collect::<BTreeSet<_>>(),
			BTreeSet::from(["email", "password", "username"])
		);
	}
}
