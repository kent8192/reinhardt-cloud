//! Logout button browser wiring.

#[cfg(any(wasm, test))]
const LOGOUT_SELECTOR: &str = ".js-dashboard-logout";
#[cfg(wasm)]
const BOUND_ATTR: &str = "data-dashboard-logout-bound";

/// Attach logout behavior to dashboard shell buttons on the current page.
#[cfg(wasm)]
pub fn ensure_logout_buttons_connected() {
	use wasm_bindgen::JsCast;
	use wasm_bindgen::closure::Closure;
	use wasm_bindgen_futures::spawn_local;

	use crate::apps::auth::server_fn::logout::logout;
	use crate::shared::client::routes::route_href;

	let Some(document) = web_sys::window().and_then(|w| w.document()) else {
		return;
	};
	let Ok(nodes) = document.query_selector_all(LOGOUT_SELECTOR) else {
		return;
	};

	for i in 0..nodes.length() {
		let Some(node) = nodes.item(i) else {
			continue;
		};
		let Ok(element) = node.dyn_into::<web_sys::Element>() else {
			continue;
		};
		if element.has_attribute(BOUND_ATTR) {
			continue;
		}
		let _ = element.set_attribute(BOUND_ATTR, "true");
		let closure =
			Closure::<dyn FnMut(web_sys::Event)>::wrap(Box::new(move |event: web_sys::Event| {
				event.prevent_default();
				spawn_local(async {
					let _ = logout().await;
					if let Some(window) = web_sys::window() {
						let _ = window
							.location()
							.set_href(&route_href("auth:login_page", "/login"));
					}
				});
			}));
		let _ = element.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref());
		closure.forget();
	}
}

/// Native rendering does not need browser event wiring.
#[cfg(not(wasm))]
pub fn ensure_logout_buttons_connected() {}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	#[rstest]
	fn logout_selector_targets_logout_button_class() {
		// Arrange / Act / Assert
		assert_eq!(super::LOGOUT_SELECTOR, ".js-dashboard-logout");
	}
}
