//! SPA router configuration for the Reinhardt Cloud WASM client.
//!
//! Routes are composed in [`init_router`], which builds a
//! [`reinhardt::urls::prelude::ClientRouter`] through
//! [`reinhardt::urls::prelude::UnifiedRouter`] and installs it globally
//! as a side effect. The terminating
//! [`UnifiedRouter::register_globally`] call installs the server router
//! (empty here — server-side routes live in `crate::config::urls`) and
//! the `ClientUrlReverser` in one step, so callers of `url_for(name)`
//! on either side can resolve every route-backed component declared
//! below.
//!
//! Global access at runtime is provided by `reinhardt::pages::with_router`,
//! which is installed by `ClientLauncher::launch()`. This module no
//! longer owns a thread-local instance.
//!
//! # Route names
//!
//! Every SPA route is registered from route-backed component metadata in
//! each app's `url_patterns()` function, so the path and name live with the
//! page component declaration. Names follow the `<app>:<name>` namespace
//! convention used by server-side view macros. SPA names use a `_page`
//! suffix where a server-side route already owns the unsuffixed name
//! (e.g. `auth:login` is the POST API, while `auth:login_page` is the
//! SPA page).
//!
//! Route handlers live with each app under `apps/<app>/client/pages`
//! so SPA pages can evolve per app without central router path drift.

use reinhardt::urls::prelude::{ClientRouter, UnifiedRouter};

use crate::apps::{auth, clusters, dashboard, deployments, github};
use crate::shared::client::pages::not_found::not_found_page;

/// Build the SPA router and register the client URL reverser globally.
///
/// Passed to `ClientLauncher::router_client(init_router)` from
/// `client.rs`. The returned [`ClientRouter`] carries the named-route
/// table that drives both SPA navigation and reverse URL lookup on the
/// WASM client.
pub fn init_router() -> ClientRouter {
	UnifiedRouter::new()
		.client(|c| c.not_found(not_found_page))
		.mount_unified("/", dashboard::urls::url_patterns())
		.mount_unified("/auth/", auth::urls::url_patterns())
		.mount_unified("/clusters/", clusters::urls::url_patterns())
		.mount_unified("/deployments/", deployments::urls::url_patterns())
		.mount_unified("/github/", github::urls::url_patterns())
		.register_globally()
}
