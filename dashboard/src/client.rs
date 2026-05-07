//! WASM client entry point for the Reinhardt Cloud dashboard.
//!
//! Bootstraps the SPA via [`reinhardt::pages::ClientLauncher`], which
//! handles router initialization, history listener wiring, the DOM
//! mount on `#app`, re-mount on every
//! [`reinhardt::pages::Router::on_navigate`] event, and built-in SPA
//! link interception. Dashboard-specific concerns (app state init,
//! toast container, WebSocket bootstrap) plug in through the launcher
//! lifecycle hooks (`before_launch`, `on_path`).
//!
//! Re-mount on navigation went through a reactive `Effect` until
//! upstream PR kent8192/reinhardt-web#4114 replaced the Effect with a
//! direct `Router::on_navigate` observer. Both APIs flow through the
//! same `on_path` hook here, so this module did not need to change.

pub mod components;
pub mod layout;
pub mod pages;
#[cfg(wasm)]
pub mod router;
pub mod state;
pub mod ws;

#[cfg(wasm)]
mod wasm_entry {
	use wasm_bindgen::prelude::*;

	use reinhardt::pages::{ClientLauncher, PathCtx};

	use super::*;

	/// WASM entry point — invoked automatically when the module loads.
	#[wasm_bindgen(start)]
	pub(super) fn main() -> Result<(), JsValue> {
		// Hedge: ClientLauncher installs the panic hook when its feature
		// is enabled; calling set_once twice is harmless.
		console_error_panic_hook::set_once();

		// Delegate router init, history listener, DOM mount, SPA link
		// interception, and re-mount on navigate to ClientLauncher.
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
