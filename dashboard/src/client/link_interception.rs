//! SPA link interception for the WASM client.
//!
//! `ClientLauncher` does not install link interception in
//! reinhardt-web v0.1.0-rc.22, so the dashboard owns this concern.
//! Remove this module once `reinhardt-pages` exposes a built-in
//! equivalent.

use reinhardt::pages::with_router;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// Attach a single global click listener that routes internal `<a>`
/// clicks through the SPA router instead of triggering full page
/// reloads. External links and `target="_blank"` anchors are left
/// untouched so the browser handles them normally.
pub fn setup_link_interception(document: &web_sys::Document) {
	let closure = Closure::wrap(Box::new(move |event: web_sys::MouseEvent| {
		let Some(target) = event.target() else { return };

		// Walk up the DOM tree to find the closest <a> ancestor.
		let mut el: Option<web_sys::Element> = target.dyn_ref::<web_sys::Element>().cloned();
		while let Some(ref element) = el {
			if element.tag_name().eq_ignore_ascii_case("A") {
				break;
			}
			el = element.parent_element();
		}
		let Some(anchor) = el else { return };

		// Only intercept internal links (those starting with "/").
		let Some(href) = anchor.get_attribute("href") else {
			return;
		};
		if !href.starts_with('/') {
			return;
		}
		if anchor.get_attribute("target").as_deref() == Some("_blank") {
			return;
		}

		event.prevent_default();
		with_router(|r| {
			let _ = r.push(&href);
		});
	}) as Box<dyn FnMut(_)>);

	document
		.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref())
		.expect("failed to add click listener");
	closure.forget();
}
