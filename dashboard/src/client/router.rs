//! SPA router configuration for the Reinhardt Cloud WASM client.
//!
//! Routes are declared as a pure function returning a `Router`. Global
//! access at runtime is provided by `reinhardt::pages::with_router`,
//! which is installed by `ClientLauncher::launch()`. This module no
//! longer owns a thread-local instance.

use reinhardt::pages::router::Router;

use crate::apps::auth::client::pages::{login_page, register_page};

use super::layout::dashboard_shell;
use super::pages::not_found_page;

/// Build the router with all application routes.
///
/// Passed to `ClientLauncher::router(init_router)` from `client.rs`.
pub fn init_router() -> Router {
	Router::new()
		.route("/", || dashboard_shell())
		.route("/login", || login_page())
		.route("/register", || register_page())
		.not_found(|| not_found_page())
}
