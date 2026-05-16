//! SPA router configuration for the Reinhardt Cloud WASM client.
//!
//! Routes are declared as a pure function returning a
//! [`reinhardt::urls::routers::ClientRouter`] built through
//! [`reinhardt::urls::routers::UnifiedRouter`]. Calling
//! [`UnifiedRouter::register_globally`] installs the server router
//! (empty here — server-side routes live in `crate::config::urls`) and
//! the `ClientUrlReverser` in one step, so callers of `url_for(name)`
//! on either side can resolve every `named_route` declared below.
//!
//! Global access at runtime is provided by `reinhardt::pages::with_router`,
//! which is installed by `ClientLauncher::launch()`. This module no
//! longer owns a thread-local instance.
//!
//! # Route names
//!
//! Every SPA route is registered via [`ClientRouter::named_route`] so that
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

use reinhardt::urls::routers::{ClientRouter, UnifiedRouter};

use crate::apps::auth::client::pages::{login_page, register_page};
use crate::apps::clusters::client::pages::clusters_list_page;
use crate::apps::dashboard::client::layout::dashboard_shell;
use crate::apps::deployments::client::pages::deployments_list_page;
use crate::shared::client::pages::not_found::not_found_page;

/// Build the SPA router and register the client URL reverser globally.
///
/// Passed to `ClientLauncher::router_client(init_router)` from
/// `client.rs`. The returned [`ClientRouter`] carries the named-route
/// table that drives both SPA navigation and `ResolvedUrls`-backed
/// reverse URL lookup on the WASM client.
pub fn init_router() -> ClientRouter {
	UnifiedRouter::new()
		.client(|c| {
			c.named_route("dashboard:home", "/", dashboard_shell)
				.named_route("auth:login_page", "/login", login_page)
				.named_route("auth:register_page", "/register", register_page)
				.named_route("clusters:list", "/clusters", clusters_list_page)
				.named_route("deployments:list", "/deployments", deployments_list_page)
				.not_found(not_found_page)
		})
		.register_globally()
}
