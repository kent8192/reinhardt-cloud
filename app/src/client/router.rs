//! SPA router configuration for the Nuages WASM client.
//!
//! Defines client-side routes and provides global router access via
//! `thread_local!` storage following the reinhardt-pages pattern.

use std::cell::RefCell;

use reinhardt::pages::component::Page;
use reinhardt::pages::page;
use reinhardt::pages::router::Router;

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
		.route("/login", || login_placeholder())
		.route("/register", || register_placeholder())
		.not_found(|| not_found_page())
}

/// Placeholder login page (replaced in Task 6).
fn login_placeholder() -> Page {
	page!(|| {
		div {
			class: "min-h-screen flex items-center justify-center bg-gray-50",
			div {
				class: "text-center",
				h2 {
					class: "text-2xl font-bold text-gray-800 mb-4",
					"Login"
				}
				p {
					class: "text-gray-600 mb-4",
					"Login page coming soon."
				}
				a {
					href: "/",
					class: "text-blue-600 hover:underline",
					"Back to Dashboard"
				}
			}
		}
	})()
}

/// Placeholder register page (replaced in Task 6).
fn register_placeholder() -> Page {
	page!(|| {
		div {
			class: "min-h-screen flex items-center justify-center bg-gray-50",
			div {
				class: "text-center",
				h2 {
					class: "text-2xl font-bold text-gray-800 mb-4",
					"Register"
				}
				p {
					class: "text-gray-600 mb-4",
					"Registration page coming soon."
				}
				a {
					href: "/",
					class: "text-blue-600 hover:underline",
					"Back to Dashboard"
				}
			}
		}
	})()
}
