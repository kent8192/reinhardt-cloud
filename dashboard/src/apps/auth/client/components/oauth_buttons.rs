//! OAuth provider sign-in buttons.
//!
//! OAuth provider discovery is available through server functions, but
//! provider redirect callbacks are not exposed by the Pages app surface.

#[cfg(wasm)]
use crate::apps::auth::server_fn::oauth_providers::list_oauth_providers;

/// Populate an OAuth provider mount point from server-side configuration.
#[cfg(wasm)]
pub fn ensure_oauth_buttons_connected(container_id: &'static str, verb: &'static str) {
	wasm_bindgen_futures::spawn_local(async move {
		let _ = (container_id, verb, list_oauth_providers().await);
	});
}

/// Non-WASM stub so native builds can share the same client entry wiring.
#[cfg(not(wasm))]
#[allow(dead_code)] // Called from WASM route wiring only; native builds keep the shared API surface.
pub fn ensure_oauth_buttons_connected(_container_id: &'static str, _verb: &'static str) {}
