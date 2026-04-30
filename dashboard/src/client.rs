//! WASM client entry point for the Reinhardt Cloud dashboard.
//!
//! Bootstraps the SPA via `reinhardt::pages::ClientLauncher`, which
//! handles router initialization, history listener wiring, the DOM
//! mount on `#app`, and the reactive re-render Effect. Dashboard-
//! specific concerns (link interception, app state, toast container,
//! WebSocket bootstrap) are layered on top before and after
//! `launch()`.

pub mod components;
pub mod layout;
#[cfg(wasm)]
pub mod link_interception;
pub mod pages;
pub mod route_table;
#[cfg(wasm)]
pub mod router;
pub mod state;
pub mod url;
pub mod ws;

#[cfg(wasm)]
mod wasm_entry {
	use wasm_bindgen::prelude::*;

	use reinhardt::pages::reactive::Effect;
	use reinhardt::pages::{ClientLauncher, with_router};

	use super::*;

	/// WASM entry point — invoked automatically when the module loads.
	#[wasm_bindgen(start)]
	pub fn main() -> Result<(), JsValue> {
		// Hedge: ClientLauncher installs the panic hook when its feature
		// is enabled; calling set_once twice is harmless.
		console_error_panic_hook::set_once();

		state::init_app_state();

		// Install the SPA link interceptor before launch so the very
		// first click is captured. ClientLauncher does not register one.
		let document = web_sys::window()
			.and_then(|w| w.document())
			.expect("no document");
		link_interception::setup_link_interception(&document);

		// Hand router init, history listener, DOM mount and the
		// reactive re-render Effect to ClientLauncher.
		ClientLauncher::new("#app")
			.router(router::init_router)
			.launch()?;

		// Path-driven side effects: ensure the toast container exists
		// and open the notifications WebSocket whenever the user is on
		// the authenticated root route. Subscribes to the same
		// current_path Signal as the launcher's render Effect.
		let path_signal = with_router(|r| r.current_path().clone());
		let doc = document.clone();
		let effect = Effect::new(move || {
			let path = path_signal.get();
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
		// Intentional leak: must persist for the WASM module's lifetime.
		std::mem::forget(effect);

		Ok(())
	}
}
