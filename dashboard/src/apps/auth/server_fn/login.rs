//! Login server function for frontend authentication.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

#[cfg(native)]
use reinhardt::core::exception::Error as AppError;

use crate::apps::auth::serializers::LoginRequest;
use crate::shared::AuthResponse;

/// Authenticate user with credentials and set session cookie.
///
/// On the server side this verifies the username and password against
/// the database, creates a Redis session, and sets an HTTP-only
/// `sessionid` cookie. The browser automatically sends this cookie
/// on subsequent requests.
#[server_fn(pre_validate = true)]
pub async fn login(
	request: LoginRequest,
	#[inject] http_request: reinhardt::pages::server_fn::ServerFnRequest,
	#[inject] settings: reinhardt::di::Depends<crate::config::ProjectSettings>,
	#[inject] session_service: reinhardt::di::Depends<crate::apps::auth::services::SessionService>,
) -> Result<AuthResponse, ServerFnError> {
	use tracing::error;

	use crate::apps::auth::services;
	use crate::shared::UserInfo;

	let user = services::verify_credentials(&request.username, &request.password)
		.await
		.map_err(server_fn_error_from_app_error)?;

	let session_id = session_service.create_session(&user).await.map_err(|err| {
		error!("Failed to create session: {err}");
		ServerFnError::application("Internal server error")
	})?;

	// Set session cookie via the SharedResponseCookies jar.
	// The server_fn router reads SharedResponseCookies after the handler
	// and applies them as Set-Cookie response headers.
	let is_debug = settings.core.debug;
	let cookie = services::session_cookie_header(&session_id, is_debug);
	http_request.add_response_cookie(cookie);

	let user_info = UserInfo::from(&user);
	Ok(AuthResponse {
		success: true,
		user: Some(user_info),
	})
}

#[cfg(native)]
fn server_fn_error_from_app_error(err: AppError) -> ServerFnError {
	match err {
		AppError::Authentication(message) => ServerFnError::server(401, message),
		AppError::Authorization(message) => ServerFnError::server(403, message),
		_ => {
			tracing::error!("verify_credentials internal error: {err}");
			ServerFnError::application("Internal server error")
		}
	}
}

#[cfg(all(test, native))]
mod tests {
	use std::collections::BTreeSet;

	use reinhardt::pages::server_fn::ServerFnErrorKind;
	use rstest::rstest;

	use super::*;

	#[rstest]
	fn test_authentication_error_becomes_unauthorized_server_error() {
		// Arrange
		let err = AppError::Authentication("Invalid credentials".to_string());

		// Act
		let server_fn_error = server_fn_error_from_app_error(err);

		// Assert
		assert_eq!(server_fn_error.message(), "Invalid credentials");
		assert_eq!(server_fn_error.kind(), ServerFnErrorKind::Server);
		assert_eq!(server_fn_error.status(), Some(401));
	}

	#[rstest]
	fn test_authorization_error_becomes_forbidden_server_error() {
		// Arrange
		let err = AppError::Authorization("Email verification required".to_string());

		// Act
		let server_fn_error = server_fn_error_from_app_error(err);

		// Assert
		assert_eq!(server_fn_error.message(), "Email verification required");
		assert_eq!(server_fn_error.kind(), ServerFnErrorKind::Server);
		assert_eq!(server_fn_error.status(), Some(403));
	}

	#[rstest]
	fn test_internal_error_uses_generic_application_error() {
		// Arrange
		let err = AppError::Internal("database unavailable".to_string());

		// Act
		let server_fn_error = server_fn_error_from_app_error(err);

		// Assert
		assert_eq!(server_fn_error.message(), "Internal server error");
		assert_eq!(server_fn_error.kind(), ServerFnErrorKind::Application);
		assert_eq!(server_fn_error.status(), None);
	}

	#[rstest]
	fn login_request_validation_preserves_structured_field_errors() {
		// Arrange
		let request = LoginRequest {
			username: String::new(),
			password: String::new(),
		};

		// Act
		let error = reinhardt::Validate::validate(&request)
			.map_err(ServerFnError::from)
			.expect_err("invalid login request should not reach credential verification");

		// Assert
		assert_eq!(error.kind(), ServerFnErrorKind::Validation);
		assert_eq!(
			error
				.field_errors()
				.iter()
				.map(|field| field.field())
				.collect::<BTreeSet<_>>(),
			BTreeSet::from(["password", "username"])
		);
	}
}
