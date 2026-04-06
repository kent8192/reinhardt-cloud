//! Logout server function for frontend session termination.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

/// Invalidate the current session and clear the session cookie.
///
/// Extracts the session ID from the request cookie, destroys the
/// session in Redis, and sets a `Max-Age=0` cookie to instruct the
/// browser to delete the `sessionid` cookie.
#[server_fn]
pub async fn logout(
	#[inject] http_request: reinhardt::pages::server_fn::ServerFnRequest,
) -> Result<bool, ServerFnError> {
	use reinhardt::http::ResponseCookies;
	use tracing::warn;

	use crate::apps::auth::services;

	// Extract session ID from the Cookie header
	let session_id = http_request
		.inner()
		.headers
		.get("Cookie")
		.and_then(|v| v.to_str().ok())
		.and_then(|cookies| {
			cookies.split(';').find_map(|pair| {
				let pair = pair.trim();
				let (name, value) = pair.split_once('=')?;
				if name.trim() == "sessionid" {
					Some(value.trim().to_string())
				} else {
					None
				}
			})
		});

	// Destroy the session in Redis if a session cookie was present
	if let Some(ref sid) = session_id {
		if let Err(e) = services::destroy_session(sid).await {
			warn!("Failed to destroy session during logout: {e}");
		}
	}

	// Clear the cookie regardless of whether destruction succeeded
	let cookie = "sessionid=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0".to_string();
	let mut response_cookies = http_request
		.inner()
		.extensions
		.remove::<ResponseCookies>()
		.unwrap_or_default();
	response_cookies.add(cookie);
	http_request.inner().extensions.insert(response_cookies);

	Ok(true)
}
