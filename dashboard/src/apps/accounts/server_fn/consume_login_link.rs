//! Confirms a Login Link and signs the browser in (SR-16, SR-17).
//!
//! The Login Link URL opens a page (`LOGIN_LINK_PAGE_PATH`); the secret is in
//! its fragment, which the browser never sends. That page shows a button, and
//! only pressing it calls this function. Consumption is therefore a POST made on
//! purpose, from the Dashboard's own origin, never a side effect of a GET: a
//! chat client, mail scanner, or browser prefetch that opens the URL fetches a
//! page and consumes nothing.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

use crate::apps::accounts::serializers::login_link::LoginLinkOutcome;
#[cfg(server)]
use crate::apps::accounts::server::context::AccountsServices;
#[cfg(server)]
use crate::apps::accounts::server::cookies::{SESSION_COOKIE, request_cookie};
#[cfg(server)]
use crate::apps::accounts::services::server::login_links::{
	ConsumeError, LoginLinkSecret, consume,
};
#[cfg(server)]
use crate::apps::accounts::services::server::sessions::SessionToken;
#[cfg(server)]
use crate::apps::accounts::services::server::users::record_last_login;
#[cfg(server)]
use reinhardt::pages::server_fn::ServerFnRequest;

/// Consume the Login Link `token` and, if it is valid, sign this browser in.
///
/// The session is created exactly as a GitHub sign-in creates one: the session
/// the browser presented is destroyed and a new token is issued
/// (`SessionService::replace`), the login is recorded, and the cookie has the
/// same attributes. Privileges are still read from the `User` row on every
/// request, so nothing about the link is remembered in the session. Every invalid
/// link, whatever the reason, answers `Rejected`.
#[server_fn]
pub async fn consume_login_link(
	token: String,
	#[inject] request: ServerFnRequest,
	#[inject] services: AccountsServices,
) -> Result<LoginLinkOutcome, ServerFnError> {
	let user = match consume(&LoginLinkSecret::from_input(&token)).await {
		Ok(user) => user,
		Err(ConsumeError::Rejected) => return Ok(LoginLinkOutcome::Rejected),
		Err(ConsumeError::Storage(error)) => {
			tracing::error!(%error, "consuming a login link failed");
			return Err(ServerFnError::server(500, "Internal server error"));
		}
	};
	record_last_login(user.id).await.map_err(|error| {
		tracing::error!(%error, "recording the last login failed after a login link");
		ServerFnError::server(500, "Internal server error")
	})?;

	let previous = request_cookie(request.inner(), SESSION_COOKIE)
		.map(|cookie| SessionToken::from_cookie(&cookie));
	let session = services
		.sessions
		.replace(user.id, previous.as_ref())
		.await
		.map_err(|error| {
			tracing::error!(%error, "issuing the session failed after a login link");
			ServerFnError::server(500, "Internal server error")
		})?;
	request.add_response_cookie(services.cookies.set(
		SESSION_COOKIE,
		session.token.expose(),
		"/",
		session.lifetime,
	));
	Ok(LoginLinkOutcome::SignedIn)
}
