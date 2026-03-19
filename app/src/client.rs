//! WASM client entry point for the Nuages dashboard.
//!
//! Initializes global state, the SPA router, and mounts the application
//! to the `#app` DOM element. Sets up link click interception and
//! popstate handling for client-side navigation.

pub mod layout;
pub mod pages;
pub mod router;
pub mod state;

use wasm_bindgen::prelude::*;
use web_sys::HtmlElement;

use reinhardt::pages::PageExt;
use reinhardt::pages::dom::Element;

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
	let root = document
		.get_element_by_id("app")
		.expect("should have #app element");

	// Clear the loading placeholder.
	root.set_inner_html("");

	// Render the current route and mount it.
	router::with_router(|r| {
		let view = r.render_current();
		let root_element = Element::new(root.clone());
		let _ = view.mount(&root_element);
	});

	// Set up SPA link click interception (event delegation on document).
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
								router::with_router(|r| {
									let _ = r.push(&href);
								});
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
	let popstate_handler = Closure::wrap(Box::new(move |_event: web_sys::Event| {
		router::with_router(|r| {
			let current_path = web_sys::window()
				.and_then(|w| w.location().pathname().ok())
				.unwrap_or_else(|| "/".to_string());
			let _ = r.replace(&current_path);
		});
	}) as Box<dyn FnMut(_)>);

	window
		.add_event_listener_with_callback("popstate", popstate_handler.as_ref().unchecked_ref())?;
	popstate_handler.forget();

	Ok(())
}
