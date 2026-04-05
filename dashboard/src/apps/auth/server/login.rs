//! Login server function for frontend authentication.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

use crate::shared::{AuthResponse, UserInfo};

/// Authenticate user with credentials and return session token.
///
/// On the server side this verifies the username and password against
/// the database, generates a JWT token, and returns both the token and
/// the authenticated user information. The WASM client stores the token
/// for use in subsequent authenticated server function calls.
#[server_fn]
pub async fn login(username: String, password: String) -> Result<AuthResponse, ServerFnError> {
	use tracing::error;

	use crate::apps::auth::services;

	let user = services::verify_credentials(&username, &password)
		.await
		.map_err(|err| {
			// Log internal errors for operational visibility while keeping
			// the client-facing message generic to prevent information leakage.
			let msg = err.to_string();
			if msg != "Invalid credentials" {
				error!("verify_credentials internal error: {msg}");
				return ServerFnError::application("Internal server error");
			}
			ServerFnError::application("Invalid credentials")
		})?;

	let token = services::create_session_token(&user).map_err(|err| {
		error!("Failed to create session token: {err}");
		ServerFnError::application("Internal server error")
	})?;

	let user_info = UserInfo::from(&user);
	Ok(AuthResponse {
		success: true,
		user: Some(user_info),
		token: Some(token),
	})
}
