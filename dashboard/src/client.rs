//! WASM client entry point for the Reinhardt Cloud dashboard.
//!
//! Bootstraps the SPA via [`reinhardt::pages::ClientLauncher`], which
//! handles router initialization, history listener wiring, the DOM
//! mount on `#app`, the reactive re-render Effect, and built-in SPA
//! link interception. Dashboard-specific concerns (app state init,
//! toast container, WebSocket bootstrap) plug in through the launcher
//! lifecycle hooks (`before_launch`, `on_path`).

pub mod components;
pub mod layout;
pub mod pages;
#[cfg(wasm)]
pub mod router;
pub mod state;
pub mod url;
pub mod ws;

#[cfg(wasm)]
mod wasm_entry {
	use wasm_bindgen::prelude::*;

	use reinhardt::pages::{ClientLauncher, PathCtx};

	use super::*;

	/// WASM entry point — invoked automatically when the module loads.
	#[wasm_bindgen(start)]
	pub fn main() -> Result<(), JsValue> {
		// Hedge: ClientLauncher installs the panic hook when its feature
		// is enabled; calling set_once twice is harmless.
		console_error_panic_hook::set_once();

		// Hand router init, history listener, DOM mount, SPA link
		// interception and the reactive re-render Effect to ClientLauncher.
		// Path-driven side effects (toast container + notifications WS)
		// run through `on_path` so they re-fire on every entry to "/".
		ClientLauncher::new("#app")
			.before_launch(state::init_app_state)
			.router(router::init_router)
			.on_path("/", |ctx: &PathCtx<'_>| {
				ctx.ensure_portal("toast-container", components::toast::toast_container);
				ws::connect_notifications();
			})
			.launch()?;

		Ok(())
	}
}
