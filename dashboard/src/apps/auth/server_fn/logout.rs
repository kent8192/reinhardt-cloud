//! Logout server function for frontend session termination.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

#[cfg(native)]
use reinhardt::di::Depends;
#[cfg(native)]
use reinhardt::pages::server_fn::ServerFnRequest;

#[cfg(native)]
use crate::apps::auth::services::{self, SessionService, SessionServiceKey};

/// Invalidate the current session and clear the session cookie.
///
/// Extracts the session ID from the request cookie, destroys the
/// session in Redis, and sets a `Max-Age=0` cookie to instruct the
/// browser to delete the `sessionid` cookie.
#[server_fn]
pub async fn logout(
	#[inject] http_request: ServerFnRequest,
	#[inject] session_service: Depends<SessionServiceKey, SessionService>,
) -> Result<bool, ServerFnError> {
	use tracing::warn;

	let session_id = http_request
		.inner()
		.headers
		.get("Cookie")
		.and_then(|v| v.to_str().ok())
		.and_then(services::session_id_from_cookie_header);

	// Destroy the session in Redis if a session cookie was present
	if let Some(ref sid) = session_id
		&& let Err(e) = session_service.destroy_session(sid).await
	{
		warn!("Failed to destroy session during logout: {e}");
	}

	// Clear the cookie regardless of whether destruction succeeded
	let cookie = "sessionid=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0".to_string();
	http_request.add_response_cookie(cookie);

	Ok(true)
}
