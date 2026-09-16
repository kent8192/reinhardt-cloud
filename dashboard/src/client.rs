//! WASM client entry point for the Reinhardt Cloud dashboard.
//!
//! Bootstraps the SPA via [`reinhardt::pages::ClientLauncher`], which owns
//! client routing, browser history, link interception, and mounting on `#app`.
//! Protected routes are prepared through their asynchronous layout guard before
//! their content mounts. Path hooks run after the destination commits, so
//! notification connections start only after the protected route is admitted.
//! Application state initialization runs in `before_launch`; the toast portal
//! and path-specific notifications use `on_path`.

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

	fn ensure_notifications_for_path(ctx: &PathCtx<'_>) {
		if ws::should_connect_notifications_for_path(ctx.path()) {
			ws::ensure_notifications_connected();
		}
	}

	/// WASM entry point — invoked automatically when the module loads.
	#[wasm_bindgen(start)]
	pub(super) fn main() -> Result<(), JsValue> {
		// Hedge: ClientLauncher installs the panic hook when its feature
		// is enabled; calling set_once twice is harmless.
		console_error_panic_hook::set_once();

		// Delegate routing, guarded route preparation, browser history,
		// link interception, and DOM mounting to ClientLauncher.
		// `router_client` consumes the one dashboard `ClientRouter`; route
		// reversal reads that active SPA router, so no separate registration
		// is needed.
		// Path-driven side effects run through `on_path` so they re-fire
		// on every entry to "/".
		ClientLauncher::new("#app")
			.before_launch(state::init_app_state)
			.router_client(router::init_router)
			.on_path("/", |ctx: &PathCtx<'_>| {
				ctx.ensure_portal("toast-container", components::toast::toast_container);
				ensure_notifications_for_path(ctx);
			})
			.on_path("/account", ensure_notifications_for_path)
			.on_path("/clusters", ensure_notifications_for_path)
			.on_path("/deployments", ensure_notifications_for_path)
			.launch()?;

		Ok(())
	}
}
