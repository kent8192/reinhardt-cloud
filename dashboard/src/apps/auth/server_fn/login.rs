//! Login server function for frontend authentication.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

use crate::shared::AuthResponse;

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
	#[inject] settings: reinhardt::di::Depends<crate::config::settings::ProjectSettings>,
	#[inject] session_service: reinhardt::di::Depends<
		crate::apps::auth::services::session::SessionService,
	>,
) -> Result<AuthResponse, ServerFnError> {
	#[cfg(native)]
	{
		use tracing::error;

		use crate::apps::auth::services;
		use crate::apps::auth::services::session::session_cookie_header;
		use crate::shared::UserInfo;

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

		let session_id = session_service.create_session(&user).await.map_err(|err| {
			error!("Failed to create session: {err}");
			ServerFnError::application("Internal server error")
		})?;

		// Set session cookie via the SharedResponseCookies jar.
		// The server_fn router reads SharedResponseCookies after the handler
		// and applies them as Set-Cookie response headers.
		let is_debug = settings.core.debug;
		let cookie = session_cookie_header(&session_id, is_debug);
		http_request.add_response_cookie(cookie);

		let user_info = UserInfo::from(&user);
		Ok(AuthResponse {
			success: true,
			user: Some(user_info),
		})
	}
	#[cfg(wasm)]
	{
		// The #[server_fn] macro replaces this body with an HTTP POST stub on
		// wasm; this branch exists only so the function compiles as a single
		// declaration on both targets.
		let _ = (username, password, http_request, settings, session_service);
		unreachable!("server_fn body is replaced on wasm")
	}
}
