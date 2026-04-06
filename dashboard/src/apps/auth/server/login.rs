//! Login server function for frontend authentication.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

use crate::shared::{AuthResponse, UserInfo};

/// Authenticate user with credentials and set session cookie.
///
/// On the server side this verifies the username and password against
/// the database, creates a Redis session, and sets an HTTP-only
/// `sessionid` cookie. The browser automatically sends this cookie
/// on subsequent requests.
#[server_fn]
pub async fn login(
	username: String,
	password: String,
	#[inject] http_request: reinhardt::pages::server_fn::ServerFnRequest,
) -> Result<AuthResponse, ServerFnError> {
	use reinhardt::http::ResponseCookies;
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

	let session_id = services::create_session(&user).await.map_err(|err| {
		error!("Failed to create session: {err}");
		ServerFnError::application("Internal server error")
	})?;

	// Set session cookie via ResponseCookies extension.
	// The server_fn router extracts ResponseCookies from request extensions
	// and applies them as Set-Cookie response headers.
	let is_debug = crate::config::settings::get_settings().core.debug;
	let secure_flag = if is_debug { "" } else { "; Secure" };
	let cookie = format!(
		"sessionid={session_id}; HttpOnly; SameSite=Lax; Path=/{secure_flag}; Max-Age=86400"
	);
	let mut response_cookies = http_request
		.inner()
		.extensions
		.remove::<ResponseCookies>()
		.unwrap_or_default();
	response_cookies.add(cookie);
	http_request.inner().extensions.insert(response_cookies);

	let user_info = UserInfo::from(&user);
	Ok(AuthResponse {
		success: true,
		user: Some(user_info),
	})
}
