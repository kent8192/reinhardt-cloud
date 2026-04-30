//! SPA router configuration for the Reinhardt Cloud WASM client.
//!
//! Routes are declared as a pure function returning a `Router`. Global
//! access at runtime is provided by `reinhardt::pages::with_router`,
//! which is installed by `ClientLauncher::launch()`. This module no
//! longer owns a thread-local instance.
//!
//! # Route names
//!
//! Every SPA route is registered via [`Router::named_route`] so that
//! [`super::url::DashboardUrlResolver`] can perform reverse lookups for
//! `href` and `redirect_on_success` values. Names follow the
//! `<app>:<name>` namespace convention used by server-side view macros.
//! SPA names use a `_page` suffix where a server-side route already owns
//! the unsuffixed name (e.g. `auth:login` is the POST API, while
//! `auth:login_page` is the SPA page).
//!
//! `dashboard:clusters` and `dashboard:deployments` are placeholder
//! entries that resolve to the not-found view until those pages are
//! implemented; they exist solely to keep navigation links resolvable
//! through the URL resolver.
//!
//! # Native parallel registration
//!
//! Server-side reverse URL resolution is registered via
//! `UnifiedRouter::client(...)` in `crate::config::urls::make_router`,
//! which calls `register_client_reverser` so server-side callers of
//! `url_for` can reverse names. Every `named_route` call below MUST
//! appear there with the same `(name, pattern)` pair.

use reinhardt::pages::router::Router;

use crate::apps::auth::client::pages::{login_page, register_page};

use super::layout::dashboard_shell;
use super::pages::not_found_page;

/// Build the router with all application routes.
///
/// Passed to `ClientLauncher::router(init_router)` from `client.rs`.
pub fn init_router() -> Router {
	Router::new()
		.named_route("dashboard:home", "/", || dashboard_shell())
		.named_route("auth:login_page", "/login", || login_page())
		.named_route("auth:register_page", "/register", || register_page())
		// Placeholder names so navigation hrefs resolve via UrlResolver
		// even before these pages are implemented.
		.named_route("dashboard:clusters", "/clusters", || not_found_page())
		.named_route("dashboard:deployments", "/deployments", || {
			not_found_page()
		})
		.not_found(|| not_found_page())
}
