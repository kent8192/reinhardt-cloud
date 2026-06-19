//! Reinhardt Project Management CLI for Reinhardt Cloud
//!
//! This is the project-specific management command interface (equivalent to Django's manage.py).
//!
//! ## Router Registration
//!
//! URL patterns are automatically registered by the framework.
//! No manual registration is required - see `src/config/urls.rs` for the
//! `#[routes]` attribute macro that enables this.
//!
//! ## Native vs. WASM
//!
//! This binary is native-only (it depends on `tokio`, `reinhardt::commands`,
//! and other server-side crates that don't link for `wasm32-unknown-unknown`).
//! The WASM build of this crate skips it via the `cfg(not(target_arch =
//! "wasm32"))` gate below. The empty wasm32 stub keeps `wasm-pack test`'s
//! `cargo build --tests` happy without dragging native deps into the wasm
//! target. Refs `kent8192/reinhardt-cloud#574`.

#[cfg(not(target_arch = "wasm32"))]
use reinhardt::commands::{CommandRegistry, execute_from_command_line_with_registry_and_settings};
#[cfg(not(target_arch = "wasm32"))]
use reinhardt_cloud_dashboard::config::management::{
	CreateApiTokenCommand, ListApiTokensCommand, RevokeApiTokenCommand, SeedSelfDeployUserCommand,
};
#[cfg(not(target_arch = "wasm32"))]
use reinhardt_cloud_dashboard::config::settings::get_settings;
#[cfg(not(target_arch = "wasm32"))]
use std::process;

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() {
	// SAFETY: Called at program start before any spawned tasks.
	// env::set_var is safe in single-threaded context.
	unsafe {
		std::env::set_var(
			"REINHARDT_SETTINGS_MODULE",
			"reinhardt_cloud_dashboard.config.settings",
		);
	}

	let mut registry = CommandRegistry::new();
	registry.register(Box::new(SeedSelfDeployUserCommand));
	registry.register(Box::new(CreateApiTokenCommand));
	registry.register(Box::new(ListApiTokensCommand));
	registry.register(Box::new(RevokeApiTokenCommand));

	if let Err(e) =
		execute_from_command_line_with_registry_and_settings(registry, get_settings()).await
	{
		eprintln!("Error: {e}");
		process::exit(1);
	}
}

/// WASM stub. The dashboard's WASM bundle is built via `cdylib` from
/// `src/lib.rs` (`#[wasm_bindgen(start)]` in `client::wasm_entry::main`),
/// not from this CLI. Refs #574.
#[cfg(target_arch = "wasm32")]
fn main() {}
