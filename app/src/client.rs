//! WASM client entry point for the Nuages dashboard.
//!
//! Renders the current route as static HTML into the `#app` element.
//! SPA navigation is handled by JavaScript in `index.html` which
//! calls the exported `navigate()` function after updating history.

pub mod layout;
pub mod pages;
pub mod state;

use wasm_bindgen::prelude::*;

/// WASM entry point — called automatically when the module loads.
#[wasm_bindgen(start)]
pub fn main() -> Result<(), JsValue> {
	console_error_panic_hook::set_once();
	state::init_app_state();
	navigate();
	Ok(())
}

/// Re-render the page based on the current URL path.
///
/// Called from JavaScript after `history.pushState()` or `popstate`.
// Workaround: reinhardt-pages reactive Effect system panics with
// "RefCell already borrowed" on non-/ routes during WASM initialization.
// See: https://github.com/kent8192/reinhardt-web/issues/2667
// Scope: client.rs, auth/client/pages/login.rs, auth/client/pages/register.rs
#[wasm_bindgen]
pub fn navigate() {
	let window = web_sys::window().expect("no global window");
	let document = window.document().expect("no document");
	let app = document
		.get_element_by_id("app")
		.expect("no #app element");

	let path = window
		.location()
		.pathname()
		.unwrap_or_else(|_| "/".to_string());

	let page = match path.as_str() {
		"/login" => crate::apps::auth::client::pages::login_page(),
		"/register" => crate::apps::auth::client::pages::register_page(),
		"/" => layout::dashboard_shell(),
		_ => pages::not_found_page(),
	};

	app.set_inner_html(&page.render_to_string());
}
