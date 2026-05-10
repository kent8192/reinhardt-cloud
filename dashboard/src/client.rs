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
	use std::collections::HashMap;

	use wasm_bindgen::prelude::*;

	use reinhardt::pages::{ClientLauncher, PathCtx};
	use reinhardt::{ClientUrlReverser, register_client_reverser};

	use super::router;
	use crate::shared::client::{components, state, ws};

	// WORKAROUND for kent8192/reinhardt-web#4230 (tracked in
	// kent8192/reinhardt-cloud#577).
	//
	// The native side wires the SPA URL reverser through
	// `crate::config::urls::make_router`'s
	// `register_client_reverser(unified.client_ref().to_reverser())`.
	// On the WASM target the `#[routes]` macro at upstream `f0dd166c`
	// emits only the consumer (`__url_resolver_support::ResolvedUrls`),
	// not the registrar — so without this manual call the first WASM
	// render of `dashboard_shell` (and `not_found_page`) panics at
	// `ResolvedUrls::from_global()` with "Global client reverser not
	// registered. Ensure the #[routes] function has been called."
	// Refs `kent8192/reinhardt-cloud#574`, upstream
	// `kent8192/reinhardt-web#4221` (7th SPA navigation regression).
	//
	// The workaround is this helper plus the `SPA_ROUTE_PATTERNS` const
	// in `client/router.rs`; both consume the same slice so drift is
	// impossible. See the full WORKAROUND block in `client/router.rs`
	// for the upstream-resolved ideal form using
	// `UnifiedRouter::new().client(|c| ...).register_globally()`.
	//
	// Remove this helper and the `register_client_url_reverser()` call
	// in `main` when reinhardt-web#4230 is resolved.
	//
	// Ideal implementation (without workaround):
	//   /* removed entirely; reverser is registered automatically by */
	//   /* `UnifiedRouter::register_globally()` in `init_router`. */
	fn register_client_url_reverser() {
		let patterns: HashMap<String, String> = router::spa_route_pattern_pairs().collect();
		register_client_reverser(ClientUrlReverser::new(patterns));
	}

	/// WASM entry point — invoked automatically when the module loads.
	#[wasm_bindgen(start)]
	pub(super) fn main() -> Result<(), JsValue> {
		// Hedge: ClientLauncher installs the panic hook when its feature
		// is enabled; calling set_once twice is harmless.
		console_error_panic_hook::set_once();

		// Must run before `dashboard_shell` (the layout for `/`) renders,
		// because the layout calls `ResolvedUrls::from_global()` which
		// panics if the reverser is unset. Refs #574.
		register_client_url_reverser();

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
