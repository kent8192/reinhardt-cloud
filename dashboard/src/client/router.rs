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
//! the typed `crate::config::urls::ResolvedUrls` accessor can resolve
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

/// Single source of truth for the SPA's `(name, pattern)` table.
///
/// Consumed by:
/// - [`init_router`] to build the runtime [`Router`] via `named_route`
/// - [`spa_route_pattern_pairs`] to feed `ClientUrlReverser` registration
///   from `client::wasm_entry::main` (#574 reverser-not-registered fix)
/// - the `tests/wasm/test_spa_navigation_smoke.rs` smoke test, which
///   imports this slice through the public `pub(crate)` surface so the
///   test patterns cannot drift from production
///
/// Drift between the runtime `Router` and the registered reverser was the
/// silent failure mode behind the original `Global client reverser not
/// registered` panic in `kent8192/reinhardt-cloud#574`; collapsing the
/// three previous duplicates into this one slice removes that risk.
pub(crate) const SPA_ROUTE_PATTERNS: &[(&str, &str)] = &[
	("dashboard:home", "/"),
	("auth:login_page", "/login"),
	("auth:register_page", "/register"),
	// Placeholder names so navigation hrefs resolve via UrlResolver
	// even before these pages are implemented.
	("dashboard:clusters", "/clusters"),
	("dashboard:deployments", "/deployments"),
];

/// Iterator-friendly view onto [`SPA_ROUTE_PATTERNS`] used by callers that
/// need owned `String` pairs (e.g. `ClientUrlReverser::new(HashMap<...>)`).
pub(crate) fn spa_route_pattern_pairs() -> impl Iterator<Item = (String, String)> + 'static {
	SPA_ROUTE_PATTERNS
		.iter()
		.map(|(name, pattern)| ((*name).to_string(), (*pattern).to_string()))
}

/// Build the router with all application routes.
///
/// Passed to `ClientLauncher::router(init_router)` from `client.rs`.
/// Routes are wired from [`SPA_ROUTE_PATTERNS`] so the table cannot
/// drift from `client::wasm_entry`'s reverser registration (#574).
pub fn init_router() -> Router {
	// Each `(name, pattern)` in `SPA_ROUTE_PATTERNS` maps to a handler
	// here. Keep the match arms in lock-step with the const slice —
	// adding an entry to the slice without an arm here will produce a
	// compile error from the exhaustive `match`.
	let mut router = Router::new();
	for (name, pattern) in SPA_ROUTE_PATTERNS {
		router = match *name {
			"dashboard:home" => router.named_route(name, pattern, dashboard_shell),
			"auth:login_page" => router.named_route(name, pattern, login_page),
			"auth:register_page" => router.named_route(name, pattern, register_page),
			"dashboard:clusters" | "dashboard:deployments" => {
				router.named_route(name, pattern, not_found_page)
			}
			other => panic!(
				"client/router.rs: SPA_ROUTE_PATTERNS entry '{other}' has no \
				 matching handler in init_router(); add an arm or remove the entry"
			),
		};
	}
	router.not_found(not_found_page)
}
