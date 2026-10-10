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
//!
//! All five are the accounts share of the enumerated unauthenticated
//! surface (SR-10, see `config::middleware::access_gate`). There is no route
//! that accepts a credential: GitHub is the only identity provider (SR-01).

use reinhardt::ServerRouter;
use reinhardt::pages::server_fn::ServerFnRouterExt;

use crate::apps::accounts::server::views;
use crate::apps::accounts::server_fn::{current_viewer, sign_out, take_sign_in_notice};

/// Prefix of the accounts HTTP endpoints.
pub const AUTH_PREFIX: &str = "/api/auth/";

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
];

pub fn server_url_patterns() -> ServerRouter {
	let auth = ServerRouter::new()
		.endpoint(views::start_github_sign_in)
		.endpoint(views::finish_github_sign_in);
	ServerRouter::new()
		.mount(AUTH_PREFIX, auth)
		.server_fn(take_sign_in_notice::take_sign_in_notice::marker)
		.server_fn(current_viewer::current_viewer::marker)
		.server_fn(sign_out::sign_out::marker)
}
