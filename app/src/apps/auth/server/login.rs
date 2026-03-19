//! Login server function for frontend authentication.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

use crate::shared::AuthResponse;

/// Authenticate user with credentials and return session token.
///
/// On the server side this verifies the username and password against
/// the database, generates a JWT token, and returns both the token and
/// the authenticated user information. The WASM client stores the token
/// for use in subsequent authenticated server function calls.
#[server_fn]
pub async fn login(username: String, password: String) -> Result<AuthResponse, ServerFnError> {
	use crate::apps::auth::services;

	let user = services::verify_credentials(&username, &password)
		.await
		.map_err(|_| ServerFnError::application("Invalid credentials"))?;

	let token = services::create_session_token(&user).map_err(ServerFnError::application)?;

	let user_info = services::user_to_info(&user);
	Ok(AuthResponse {
		success: true,
		user: Some(user_info),
		token: Some(token),
	})
}
