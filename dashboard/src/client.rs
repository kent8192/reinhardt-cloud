//! WASM client entry point for the Reinhardt Cloud dashboard.
//!
//! Renders the current route reactively into the `#app` element using
//! the reinhardt-pages Router and Effect system. SPA navigation is
//! handled entirely in WASM — no inline JavaScript is required.

pub mod components;
pub mod layout;
pub mod pages;
#[cfg(wasm)]
pub mod router;
pub mod state;
pub mod ws;

#[cfg(wasm)]
mod wasm_entry {
	use wasm_bindgen::JsCast;
	use wasm_bindgen::closure::Closure;
	use wasm_bindgen::prelude::*;

	use reinhardt::pages::reactive::Effect;

	use super::*;

	/// WASM entry point — called automatically when the module loads.
	#[wasm_bindgen(start)]
	pub fn main() -> Result<(), JsValue> {
		console_error_panic_hook::set_once();
		state::init_app_state();

		// Initialize the SPA router and register browser history listener
		router::init_global_router();
		router::with_router(|r| r.setup_history_listener());

		let window = web_sys::window().expect("no global window");
		let document = window.document().expect("no document");

		// Set up global click handler for SPA link interception
		setup_link_interception(&document);

		// Set up reactive rendering — re-renders #app when route changes
		let path_signal = router::with_router(|r| r.current_path().clone());
		let doc = document.clone();
		let effect = Effect::new(move || {
			// Subscribe to path signal to trigger re-render on route change
			let path = path_signal.get();
			let app = doc.get_element_by_id("app").expect("no #app element");
			let page = router::with_router(|r| r.render_current());
			app.set_inner_html(&page.render_to_string());

			// Initialize toast container and WebSocket on authenticated pages.
			// Guard against duplicates across re-renders by checking for existing elements.
			if path == "/" {
				if doc.get_element_by_id("toast-container").is_none() {
					let toast_html = components::toast::toast_container().render_to_string();
					let toast_div = doc.create_element("div").unwrap();
					toast_div.set_inner_html(&toast_html);
					if let Some(child) = toast_div.first_element_child() {
						doc.body().unwrap().append_child(&child).unwrap();
					}
				}
				ws::connect_notifications();
			}
		});
		// Keep the effect alive for the lifetime of the page
		std::mem::forget(effect);

		Ok(())
	}

	/// Intercept clicks on internal `<a>` tags and route them through the SPA router
	/// instead of triggering full page reloads.
	fn setup_link_interception(document: &web_sys::Document) {
		let closure = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
			let target = match event.target() {
				Some(t) => t,
				None => return,
			};

			// Walk up the DOM tree to find the closest <a> element
			let mut el: Option<web_sys::Element> = target.dyn_ref::<web_sys::Element>().cloned();
			while let Some(ref element) = el {
				if element.tag_name().eq_ignore_ascii_case("A") {
					break;
				}
				el = element.parent_element();
			}

			let Some(anchor) = el else { return };

			// Only intercept internal links (starting with "/")
			let Some(href) = anchor.get_attribute("href") else {
				return;
			};
			if !href.starts_with('/') {
				return;
			}

			// Skip external links marked with data-external or target="_blank"
			if anchor.get_attribute("target").as_deref() == Some("_blank") {
				return;
			}

			event.prevent_default();
			router::with_router(|r| {
				let _ = r.push(&href);
			});
		}) as Box<dyn FnMut(_)>);

		document
			.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
			.expect("failed to add click listener");
		closure.forget();
	}
}
