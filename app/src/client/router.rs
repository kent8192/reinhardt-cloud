//! SPA router configuration for the Nuages WASM client.
//!
//! Defines client-side routes and provides global router access via
//! `thread_local!` storage following the reinhardt-pages pattern.

use std::cell::RefCell;

use reinhardt::pages::component::Page;
use reinhardt::pages::page;
use reinhardt::pages::router::Router;

use crate::apps::auth::client::pages::{login_page, register_page};

use super::layout::dashboard_shell;
use super::pages::not_found_page;

thread_local! {
	static ROUTER: RefCell<Option<Router>> = const { RefCell::new(None) };
}

/// Initialize the global router instance. Must be called once at startup.
pub fn init_global_router() {
	ROUTER.with(|r| {
		*r.borrow_mut() = Some(init_router());
	});
}

/// Access the global router within a closure.
///
/// # Panics
///
/// Panics if `init_global_router` has not been called.
pub fn with_router<F, R>(f: F) -> R
where
	F: FnOnce(&Router) -> R,
{
	ROUTER.with(|r| {
		f(r.borrow()
			.as_ref()
			.expect("Router not initialized. Call init_global_router() first."))
	})
}

/// Build the router with all application routes.
fn init_router() -> Router {
	Router::new()
		.route("/", || dashboard_shell())
		.route("/login", || login_page())
		.route("/register", || register_page())
		.not_found(|| not_found_page())
}
