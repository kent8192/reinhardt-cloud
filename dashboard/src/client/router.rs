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
//! `clusters:list` and `deployments:list` currently resolve to a
//! placeholder page that delegates to the shared 404 view; the route
//! handlers live with each app under
//! `apps/<app>/client/pages/list.rs` so the SPA pages can be filled in
//! per app without further refactoring of this file.
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
use crate::apps::clusters::client::pages::clusters_list_page;
use crate::apps::deployments::client::pages::deployments_list_page;

use super::layout::dashboard_shell;
use super::pages::not_found_page;

// ──────────────────────────────────────────────────────────────────────
// WORKAROUND for kent8192/reinhardt-web#4230 (tracked in
// kent8192/reinhardt-cloud#577).
//
// The framework currently splits SPA routing into two parallel concrete
// types — `reinhardt::pages::router::Router` (consumed by
// `ClientLauncher::router(...)`, with reactive observation fields) and
// `reinhardt::urls::ClientRouter` (returned by `UnifiedRouter::client(...)`,
// without observation). There is no `From<ClientRouter>` impl, so
// `UnifiedRouter::register_globally()` cannot feed `ClientLauncher`. The
// `#[routes]` macro at `f0dd166c` further only emits
// `register_client_reverser` on the native target, leaving the WASM client
// to register the reverser by hand. This forces this crate to maintain a
// **parallel route table**: once for the SPA's `pages::Router`
// (`init_router` below) and once for the URL reverser
// (`client::wasm_entry::register_client_url_reverser` in `client.rs`).
// Drift between the two re-introduces the `Global client reverser not
// registered` panic that #574 chased through 7 iterations of
// `kent8192/reinhardt-web#4221`.
//
// The workaround is the `SPA_ROUTE_PATTERNS` const + handler-dispatch
// `init_router` in this file plus the `register_client_url_reverser`
// helper in `client.rs::wasm_entry`. Both consume the same const slice
// so drift is impossible while the workaround is in place.
//
// Remove this workaround when reinhardt-web#4230 is resolved (the
// framework collapses `pages::Router` into `urls::ClientRouter`,
// `ClientLauncher::router(...)` accepts the unified type, and the
// `#[routes]` macro emits `register_client_reverser` on WASM).
//
// Ideal implementation (without workaround):
//   pub fn init_router() -> reinhardt::urls::UnifiedRouter {
//       use reinhardt::urls::UnifiedRouter;
//       UnifiedRouter::new()
//           .client(|c| {
//               c.named_route("dashboard:home", "/", dashboard_shell)
//                   .named_route("auth:login_page", "/login", login_page)
//                   .named_route("auth:register_page", "/register", register_page)
//                   .named_route("clusters:list", "/clusters", clusters_list_page)
//                   .named_route("deployments:list", "/deployments", deployments_list_page)
//                   .not_found(not_found_page)
//           })
//           .register_globally()
//   }
//
// Caller in `client.rs::wasm_entry::main` becomes:
//   ClientLauncher::new("#app")
//       .router(router::init_router)  // returns ClientRouter via register_globally()
//       .launch()?;
//
// `client.rs::wasm_entry::register_client_url_reverser` and the const
// slice / `spa_route_pattern_pairs` helper below all disappear.
// ──────────────────────────────────────────────────────────────────────

/// Single source of truth for the SPA's `(name, pattern)` table.
///
/// Consumed by [`init_router`] (to build `pages::Router` via
/// `named_route`) and [`spa_route_pattern_pairs`] (to feed
/// `ClientUrlReverser` registration from `client::wasm_entry::main`).
/// See the WORKAROUND block above for why this duplication is necessary
/// today and the ideal `UnifiedRouter::register_globally()` form that
/// replaces it after `kent8192/reinhardt-web#4230` lands.
pub(crate) const SPA_ROUTE_PATTERNS: &[(&str, &str)] = &[
	("dashboard:home", "/"),
	("auth:login_page", "/login"),
	("auth:register_page", "/register"),
	("clusters:list", "/clusters"),
	("deployments:list", "/deployments"),
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
			"clusters:list" => router.named_route(name, pattern, clusters_list_page),
			"deployments:list" => router.named_route(name, pattern, deployments_list_page),
			other => panic!(
				"client/router.rs: SPA_ROUTE_PATTERNS entry '{other}' has no \
				 matching handler in init_router(); add an arm or remove the entry"
			),
		};
	}
	router.not_found(not_found_page)
}
