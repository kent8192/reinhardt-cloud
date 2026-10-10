//! WASM entry point for cloud_control_plane.
//!
//! Bootstraps the SPA via `ClientLauncher::register_routes_from_inventory` and
//! installs the page i18n context (see `crate::i18n`) so `t!` resolves from the
//! first render.
//! Each app exposes its own client router through `apps/<app>/urls.rs`;
//! combine them in
//! `src/config/urls.rs` so the `#[routes]` registration owns both server and
//! client route aggregation.

use reinhardt::pages::ClientLauncher;
use wasm_bindgen::JsValue;

use crate::i18n::i18n_context;

// The `msw` feature is only enabled for `wasm-pack test`. A start function
// exported as `main` would collide with the `main` symbol of the generated test
// harness, so the browser tests build the library without it.
#[cfg_attr(not(feature = "msw"), wasm_bindgen::prelude::wasm_bindgen(start))]
pub fn main() -> Result<(), JsValue> {
	ClientLauncher::new("#root")
		.i18n_context(i18n_context())
		.register_routes_from_inventory()
		.launch()
}
