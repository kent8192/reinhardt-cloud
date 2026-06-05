//! WASM client entry point for the Reinhardt Cloud dashboard.
//!
//! Bootstraps the SPA via [`reinhardt::pages::ClientLauncher`], which
//! handles router initialization, history listener wiring, the DOM
//! mount on `#app`, re-mount on every
//! [`reinhardt::pages::Router::on_navigate`] event, and built-in SPA
//! link interception. Dashboard-specific concerns (app state init,
//! toast container) plug in through the launcher lifecycle hooks
//! (`before_launch`, `on_path`).
//!
//! Re-mount on navigation went through a reactive `Effect` until
//! upstream PR kent8192/reinhardt-web#4114 replaced the Effect with a
//! direct `Router::on_navigate` observer. Both APIs flow through the
//! same `on_path` hook here, so this module did not need to change.

#[cfg(wasm)]
pub mod router;

// `#[wasm_bindgen(start)]` registers a `main` entry that runs when the
// WASM module loads. We compile that entry out of the test build because
// `wasm-bindgen-test` injects its own `main` for the test runner, and
// `wasm-ld` discards both when two `main` exports collide ("main symbol
// is missing, may be because there are multiple exports with the same
// name but different signatures").
//
// `cfg(not(test))` won't help here — the lib is compiled without the
// `cfg(test)` flag when it's a dependency of `tests/wasm.rs`. Instead,
// we negate the `wasm-spa-test` feature: the production WASM bundle
// (built without that feature) keeps the `wasm_bindgen(start)` entry,
// and `wasm-pack test --features wasm-spa-test` opts it out. Refs
// `kent8192/reinhardt-cloud#574`.
#[cfg(all(wasm, not(feature = "wasm-spa-test")))]
mod wasm_entry {
	use wasm_bindgen::prelude::*;

	use reinhardt::pages::{ClientLauncher, PathCtx};

	use super::router;
	use crate::shared::client::{components, state, ws};

	/// WASM entry point — invoked automatically when the module loads.
	#[wasm_bindgen(start)]
	pub(super) fn main() -> Result<(), JsValue> {
		// Hedge: ClientLauncher installs the panic hook when its feature
		// is enabled; calling set_once twice is harmless.
		console_error_panic_hook::set_once();

		// Delegate router init, history listener, DOM mount, SPA link
		// interception, and re-mount on navigate to ClientLauncher.
		// `router_client` consumes `init_router`'s `ClientRouter` (built
		// via `UnifiedRouter::register_globally()`), which also installs
		// the `ClientUrlReverser` — no separate reverser registration is
		// needed.
		// Path-driven side effects run through `on_path` so they re-fire
		// on every entry to "/".
		ClientLauncher::new("#app")
			.before_launch(state::init_app_state)
			.before_launch(ws::ensure_notifications_connected)
			.router_client(router::init_router)
			.on_path("/", |ctx: &PathCtx<'_>| {
				ctx.ensure_portal("toast-container", components::toast::toast_container);
			})
			.launch()?;

		Ok(())
	}
}
