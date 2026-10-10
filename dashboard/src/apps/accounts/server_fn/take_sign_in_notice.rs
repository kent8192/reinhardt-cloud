//! Redeems the notice the GitHub callback left for this browser.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};

use crate::apps::accounts::serializers::sign_in::SignInNotice;
#[cfg(server)]
use crate::apps::accounts::server::context::AccountsServices;
#[cfg(server)]
use crate::apps::accounts::server::cookies::{NOTICE_COOKIE, request_cookie};
#[cfg(server)]
use crate::apps::accounts::services::server::sign_in_notices::NoticeKind;
#[cfg(server)]
use reinhardt::pages::server_fn::ServerFnRequest;

/// Return the notice for the last failed sign-in of this browser, once.
///
/// The notice is deleted as it is read, and its cookie is cleared, so a reload
/// shows a clean page. Anyone may call this: it only ever returns what the
/// caller's own browser was told to bring.
#[server_fn]
pub async fn take_sign_in_notice(
	#[inject] request: ServerFnRequest,
	#[inject] services: AccountsServices,
) -> Result<Option<SignInNotice>, ServerFnError> {
	let Some(id) = request_cookie(request.inner(), NOTICE_COOKIE) else {
		return Ok(None);
	};
	request.add_response_cookie(services.cookies.clear(NOTICE_COOKIE, "/"));
	let stored = services.notices.take(&id).await.map_err(|error| {
		tracing::error!(%error, "reading a sign-in notice failed");
		ServerFnError::server(500, "Internal server error")
	})?;
	Ok(stored.map(|stored| match (stored.kind, stored.login) {
		(NoticeKind::NotInvited, Some(login)) => SignInNotice::NotInvited { login },
		_ => SignInNotice::Failed,
	}))
}
