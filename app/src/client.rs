//! WASM client entry point for the Nuages dashboard.
//!
//! Initializes global state, the SPA router, and mounts the application
//! to the `#app` DOM element. Sets up a reactive Effect for re-rendering
//! on route changes, and installs link click interception and popstate
//! handling for client-side navigation.
//!
//! Uses `render_to_string()` + `set_inner_html()` instead of `mount()`
//! to avoid re-entrant RefCell borrows in the reactive system when
//! route handlers use the `form!` macro.

pub mod layout;
pub mod pages;
pub mod router;
pub mod state;

use wasm_bindgen::prelude::*;
use web_sys::HtmlElement;

/// WASM entry point — called automatically when the module loads.
#[wasm_bindgen(start)]
pub fn main() -> Result<(), JsValue> {
	// Better panic messages in the browser console.
	console_error_panic_hook::set_once();

	// Initialize global application state.
	state::init_app_state();

	// Initialize the SPA router with all routes.
	router::init_global_router();

	// Obtain the root DOM element.
	let window = web_sys::window().expect("no global `window` exists");
	let document = window.document().expect("should have a document on window");
	let app_element = document
		.get_element_by_id("app")
		.expect("should have #app element");

	// Initial render: produce static HTML string and inject it.
	// This avoids creating reactive nodes (Effects) that would cause
	// re-entrant RefCell borrows when form! macro pages are rendered.
	render_current_route(&app_element);

	// Set up SPA link click interception (event delegation on document).
	let app_for_links = app_element.clone();
	let link_handler = Closure::wrap(Box::new(move |event: web_sys::Event| {
		if let Some(target) = event.target() {
			if let Ok(element) = target.dyn_into::<HtmlElement>() {
				// Walk up from clicked element to find nearest <a>.
				let mut current = Some(element);
				while let Some(el) = current {
					if el.tag_name().to_lowercase() == "a" {
						if let Some(href) = el.get_attribute("href") {
							// Only intercept internal links.
							if href.starts_with('/') {
								event.prevent_default();
								// Update browser history.
								if let Some(w) = web_sys::window() {
									let _ = w.history().unwrap().push_state_with_url(
										&JsValue::NULL,
										"",
										Some(&href),
									);
								}
								// Re-render for the new route.
								router::with_router(|r| {
									let _ = r.push(&href);
								});
								render_current_route(&app_for_links);
								return;
							}
						}
						break;
					}
					current = el
						.parent_element()
						.and_then(|p| p.dyn_into::<HtmlElement>().ok());
				}
			}
		}
	}) as Box<dyn FnMut(_)>);

	document.add_event_listener_with_callback("click", link_handler.as_ref().unchecked_ref())?;
	link_handler.forget();

	// Handle browser back/forward navigation.
	let app_for_popstate = app_element.clone();
	let popstate_handler = Closure::wrap(Box::new(move |_event: web_sys::Event| {
		router::with_router(|r| {
			let current_path = web_sys::window()
				.and_then(|w| w.location().pathname().ok())
				.unwrap_or_else(|| "/".to_string());
			let _ = r.replace(&current_path);
		});
		render_current_route(&app_for_popstate);
	}) as Box<dyn FnMut(_)>);

	window
		.add_event_listener_with_callback("popstate", popstate_handler.as_ref().unchecked_ref())?;
	popstate_handler.forget();

	Ok(())
}

/// Render the current route to static HTML and inject into the app element.
///
/// Uses `render_to_string()` to avoid creating live reactive nodes that
/// would conflict with the router's internal signal management.
fn render_current_route(app_element: &web_sys::Element) {
	let html = router::with_router(|r| r.render_current().render_to_string());
	app_element.set_inner_html(&html);
}
