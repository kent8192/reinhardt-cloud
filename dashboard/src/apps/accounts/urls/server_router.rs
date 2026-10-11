//! Server-side URL configuration for the accounts application.
//!
//! This router is aggregated by url_patterns() and then merged by the
//! project-level config/urls.rs registration.
//!
//! | Route | Purpose | Anonymous callers |
//! |-------|---------|-------------------|
//! | `GET /api/auth/github/` | start a GitHub sign-in | allowed |
//! | `GET /api/auth/github/callback/` | complete a GitHub sign-in | allowed |
//! | `take_sign_in_notice` | sign-in page redeems its notice | allowed |
//! | `current_viewer` | who is signed in (`null` when nobody) | allowed |
//! | `sign_out` | destroy the session | allowed (idempotent) |
//! | `consume_login_link` | confirm a Login Link and sign in | allowed (the link is the credential) |
//!
//! All six are the accounts share of the enumerated unauthenticated
//! surface (SR-10, see `config::middleware::access_gate`). No route accepts a
//! password or any credential of our own: GitHub is the only identity provider
//! (SR-01), and a Login Link is a single-use grant a host operator issued with
//! `manage create-login-link` (SR-18), not something a request can obtain.
//! Nothing in this router issues a Login Link, grants Staff, or re-points a User
//! (SR-18, SR-20, SR-107): those exist only as `manage` commands.

use reinhardt::ServerRouter;
use reinhardt::pages::server_fn::ServerFnRouterExt;

use crate::apps::accounts::server::views;
use crate::apps::accounts::server_fn::{
	consume_login_link, current_viewer, sign_out, take_sign_in_notice,
};
use crate::apps::accounts::urls::paths::AUTH_PREFIX;

/// Paths (routes the framework serves) that answer anonymous callers.
///
/// `sign_out` is among them because it only ever acts on the session the caller
/// presents: a visitor whose session already expired can still leave cleanly.
///
/// Server functions are registered under `/api/server_fn/<name>`.
pub const ANONYMOUS_PATHS: &[&str] = &[
	"/api/auth/github/",
	"/api/auth/github/callback/",
	"/api/server_fn/take_sign_in_notice",
	"/api/server_fn/current_viewer",
	"/api/server_fn/sign_out",
	"/api/server_fn/consume_login_link",
];

/// Paths that start a session, so a state-changing request to them must prove
/// same-origin intent (SR-12) even when it carries no session cookie yet.
///
/// The cross-site guard only demands proof when the request carries the ambient
/// session cookie. Confirming a Login Link signs the browser in, which a hostile
/// page could otherwise trigger (a login CSRF that leaves the visitor signed in
/// as the attacker's User); requiring the `Origin` here closes that without any
/// cookie being present.
pub const SESSION_ESTABLISHING_PATHS: &[&str] = &["/api/server_fn/consume_login_link"];

pub fn server_url_patterns() -> ServerRouter {
	let auth = ServerRouter::new()
		.endpoint(views::start_github_sign_in)
		.endpoint(views::finish_github_sign_in);
	ServerRouter::new()
		.mount(AUTH_PREFIX, auth)
		.server_fn(take_sign_in_notice::take_sign_in_notice::marker)
		.server_fn(current_viewer::current_viewer::marker)
		.server_fn(sign_out::sign_out::marker)
		.server_fn(consume_login_link::consume_login_link::marker)
}
