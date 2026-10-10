//! HTTP endpoints of the accounts application.
//!
//! Both are browser navigations, not API calls, so they answer with redirects:
//!
//! - `GET /api/auth/github/` starts a sign-in and sends the browser to GitHub;
//! - `GET /api/auth/github/callback/` completes it and sends the browser on to
//!   the Dashboard (signed in) or back to the sign-in page (with a notice).
//!
//! Neither reveals why a sign-in failed beyond the notice the page shows, and
//! neither echoes anything GitHub or the visitor sent.

use reinhardt::di::params::{CookieName, CookieNamed};
use reinhardt::http::ViewResult;
use reinhardt::{Query, Response, get};
use serde::Deserialize;

use crate::apps::accounts::server::context::AccountsServices;
use crate::apps::accounts::server::cookies::{
	BINDING_COOKIE, BINDING_LIFETIME, NOTICE_COOKIE, SESSION_COOKIE,
};
use crate::apps::accounts::services::server::sessions::SessionToken;
use crate::apps::accounts::services::server::sign_in::{CallbackRequest, SignInOutcome};
use crate::apps::accounts::services::server::sign_in_notices::{
	NOTICE_LIFETIME, NoticeKind, StoredNotice,
};

/// Where the browser goes once signed in.
pub const SIGNED_IN_PATH: &str = "/";
/// The sign-in page.
pub const SIGN_IN_PAGE_PATH: &str = "/sign-in/";
/// Path scope of the binding cookie: only the callback needs it.
pub const BINDING_COOKIE_PATH: &str = "/api/auth/github/callback/";

/// Marker for the session cookie extractor.
pub struct SessionCookie;

impl CookieName for SessionCookie {
	const NAME: &'static str = SESSION_COOKIE;
}

/// Marker for the binding cookie extractor.
pub struct BindingCookie;

impl CookieName for BindingCookie {
	const NAME: &'static str = BINDING_COOKIE;
}

/// Query of the GitHub callback. Every field is optional: GitHub sends `error`
/// (ignored here, with its text) instead of `code` when the visitor declines,
/// and a forged request may send anything.
#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
	code: Option<String>,
	state: Option<String>,
}

/// Start a sign-in with GitHub.
///
/// Creates single-use, PKCE-protected state bound to this browser through the
/// binding cookie, then redirects to GitHub's authorization page.
#[get("/github/", name = "github-sign-in")]
pub async fn start_github_sign_in(#[inject] services: AccountsServices) -> ViewResult<Response> {
	let Some(sign_in) = services.sign_in.as_ref() else {
		tracing::error!("sign-in was requested but no GitHub App is configured");
		return unavailable(&services).await;
	};
	let started = match sign_in.github().begin().await {
		Ok(started) => started,
		Err(error) => {
			tracing::error!(%error, "starting a sign-in failed");
			return unavailable(&services).await;
		}
	};
	let binding_cookie = services.cookies.set(
		BINDING_COOKIE,
		&started.binding,
		BINDING_COOKIE_PATH,
		BINDING_LIFETIME,
	);
	Ok(
		no_store(Response::temporary_redirect(started.authorization_url))
			.append_header("Set-Cookie", &binding_cookie),
	)
}

/// Complete a sign-in with GitHub.
#[get("/github/callback/", name = "github-sign-in-callback")]
pub async fn finish_github_sign_in(
	Query(query): Query<CallbackQuery>,
	session: CookieNamed<SessionCookie, Option<String>>,
	binding: CookieNamed<BindingCookie, Option<String>>,
	#[inject] services: AccountsServices,
) -> ViewResult<Response> {
	let clear_binding = services.cookies.clear(BINDING_COOKIE, BINDING_COOKIE_PATH);
	let Some(sign_in) = services.sign_in.as_ref() else {
		return unavailable(&services).await;
	};

	let session = session.into_inner();
	let binding = binding.into_inner();
	let outcome = sign_in
		.complete(CallbackRequest {
			code: query.code.as_deref(),
			state: query.state.as_deref(),
			binding: binding.as_deref(),
			previous_session: session.as_deref().map(SessionToken::from_cookie),
		})
		.await;

	let response = match outcome {
		SignInOutcome::SignedIn { session } => {
			let cookie = services.cookies.set(
				SESSION_COOKIE,
				session.token.expose(),
				"/",
				session.lifetime,
			);
			no_store(Response::temporary_redirect(SIGNED_IN_PATH))
				.append_header("Set-Cookie", &cookie)
		}
		SignInOutcome::NotInvited { login } => {
			notice_redirect(
				&services,
				StoredNotice {
					kind: NoticeKind::NotInvited,
					login: Some(login),
				},
			)
			.await
		}
		SignInOutcome::Failed => {
			notice_redirect(
				&services,
				StoredNotice {
					kind: NoticeKind::Failed,
					login: None,
				},
			)
			.await
		}
	};
	Ok(response.append_header("Set-Cookie", &clear_binding))
}

/// Send the visitor back to the sign-in page, carrying `notice` in a cookie.
async fn notice_redirect(services: &AccountsServices, notice: StoredNotice) -> Response {
	let response = no_store(Response::temporary_redirect(SIGN_IN_PAGE_PATH));
	match services.notices.put(&notice).await {
		Ok(id) => response.append_header(
			"Set-Cookie",
			&services
				.cookies
				.set(NOTICE_COOKIE, &id, "/", NOTICE_LIFETIME),
		),
		Err(error) => {
			// The page still opens; it just has nothing to explain.
			tracing::error!(%error, "storing a sign-in notice failed");
			response
		}
	}
}

/// The generic outcome when sign-in cannot run at all.
async fn unavailable(services: &AccountsServices) -> ViewResult<Response> {
	Ok(notice_redirect(
		services,
		StoredNotice {
			kind: NoticeKind::Failed,
			login: None,
		},
	)
	.await)
}

fn no_store(response: Response) -> Response {
	response.with_header("Cache-Control", "no-store")
}
