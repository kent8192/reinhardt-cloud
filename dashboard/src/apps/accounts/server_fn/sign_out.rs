//! Signs the User out.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

#[cfg(server)]
use crate::apps::accounts::server::context::AccountsServices;
#[cfg(server)]
use crate::apps::accounts::server::cookies::{SESSION_COOKIE, request_cookie};
#[cfg(server)]
use crate::apps::accounts::services::server::sessions::SessionToken;
#[cfg(server)]
use crate::audit::{ActorKind, AuditEvent, Outcome};
#[cfg(server)]
use reinhardt::pages::server_fn::ServerFnRequest;

/// Destroy the caller's session on the server and clear the session cookie.
///
/// The session is deleted from Redis, not just forgotten by the browser, so a
/// copied cookie stops working too (SR-08). Signing out without a session is
/// not an error.
#[server_fn]
pub async fn sign_out(
	#[inject] request: ServerFnRequest,
	#[inject] services: AccountsServices,
) -> Result<(), ServerFnError> {
	if let Some(cookie) = request_cookie(request.inner(), SESSION_COOKIE) {
		let destroyed = services
			.sessions
			.destroy(&SessionToken::from_cookie(&cookie))
			.await
			.map_err(|error| {
				tracing::error!(%error, "destroying a session failed during sign-out");
				ServerFnError::server(500, "Internal server error")
			})?;
		if let Some(user) = destroyed {
			AuditEvent::new(
				"accounts.sign_out.succeeded",
				ActorKind::User,
				Outcome::Succeeded,
			)
			.actor_user(user)
			.subject_user(user)
			.emit();
		}
	}
	request.add_response_cookie(services.cookies.clear(SESSION_COOKIE, "/"));
	Ok(())
}
