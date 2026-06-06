//! Integration tests for the dashboard server entrypoint.
//!
//! These tests verify the regression captured in
//! kent8192/reinhardt-cloud#478: launching the dashboard via
//! `RunServerCommand::execute()` directly previously failed with
//! `No router registered` because the inventory walk performed by
//! `execute_from_command_line()` was being skipped.
//!
//! We exercise the helper that `src/main.rs` now calls in its hot
//! path so the regression cannot reappear silently.

// Native-only — see `tests/wasm.rs` for browser tests. Refs #574.
#![cfg(not(target_arch = "wasm32"))]

use reinhardt::urls::routers::{clear_router, is_router_registered};
use reinhardt::{clear_websocket_router, get_websocket_router, reverse_websocket_url};
use reinhardt_cloud_dashboard::server;
use rstest::rstest;
use serial_test::serial;

struct EnvVarGuard {
	key: &'static str,
	previous: Option<String>,
}

impl EnvVarGuard {
	fn set(key: &'static str, value: &str) -> Self {
		let previous = std::env::var(key).ok();
		// SAFETY: these tests run in the `router_global` serial group before
		// spawning server tasks, so no concurrent test in this file reads or
		// mutates the same environment variable.
		unsafe {
			std::env::set_var(key, value);
		}
		Self { key, previous }
	}
}

impl Drop for EnvVarGuard {
	fn drop(&mut self) {
		// SAFETY: this guard restores process environment during serial test
		// cleanup, before any server task has been spawned.
		unsafe {
			match &self.previous {
				Some(value) => std::env::set_var(self.key, value),
				None => std::env::remove_var(self.key),
			}
		}
	}
}

/// `register_router_from_inventory` is what the binary's
/// `server::run` calls before `RunServerCommand::execute`. Asserting
/// it on its own (rather than spawning the full server) lets the
/// test stay deterministic and fast while still catching the
/// `No router registered` regression directly: if the inventory walk
/// fails, this returns `Err` and the assertion below fails.
#[rstest]
#[tokio::test]
#[serial(router_global)]
async fn register_router_from_inventory_populates_global_router_slot()
-> Result<(), Box<dyn std::error::Error>> {
	// Arrange — the global router slot is process-wide. A previous
	// test may have left a router registered (or another integration
	// test running serially in the same group), so reset to a clean
	// "no router" state before exercising the helper.
	let _env = EnvVarGuard::set("REINHARDT_ENV", "ci");
	clear_router();
	assert!(
		!is_router_registered(),
		"precondition: global router slot must be empty before invoking the inventory walk"
	);

	// Act — drive the same code path that `dashboard/src/main.rs` runs
	// inside the container ENTRYPOINT. If the dashboard's `#[routes]`
	// inventory entry is missing or duplicated this returns `Err` and
	// the test fails with a deterministic, debuggable message.
	server::register_router_from_inventory().await?;

	// Assert — the helper must have populated the global router slot
	// that `RunServerCommand::execute` reads on startup. Without this
	// step the binary fails with "Execution error: No router registered."
	assert!(
		is_router_registered(),
		"register_router_from_inventory must register a router; \
		 see kent8192/reinhardt-cloud#478"
	);

	// Cleanup — leave the global slot empty for the next serial test.
	clear_router();

	Ok(())
}

/// `server::run` also needs to register the dashboard's WebSocket routes
/// before `RunServerCommand::execute`, otherwise `/ws/notifications` falls
/// through to the pages fallback and the browser sees an HTTP 200 response
/// instead of a WebSocket upgrade.
#[rstest]
#[tokio::test]
#[serial(router_global)]
async fn init_websocket_routes_registers_notifications_endpoint()
-> Result<(), Box<dyn std::error::Error>> {
	// Arrange
	clear_websocket_router().await;
	assert!(
		get_websocket_router().await.is_none(),
		"precondition: global WebSocket router slot must be empty"
	);

	// Act
	reinhardt_cloud_dashboard::config::urls::init_websocket_routes().await;

	// Assert
	let router = get_websocket_router()
		.await
		.expect("init_websocket_routes must register a WebSocket router");
	assert!(
		router.has_route("/ws/notifications").await,
		"WebSocket router must serve /ws/notifications; \
		 see kent8192/reinhardt-cloud#655"
	);
	assert_eq!(
		reverse_websocket_url(&router, "websocket:notifications").await,
		Some("/ws/notifications".to_string()),
		"named notifications route must reverse to the served WebSocket path"
	);

	// Cleanup
	clear_websocket_router().await;

	Ok(())
}

/// The management `runserver` path reaches WebSocket setup through a
/// runserver startup hook, not through `server::run`. Keep the hook
/// inventory-visible so local development does not regress to the SPA
/// fallback returning HTTP 200 for `/ws/notifications`. Refs #666.
#[rstest]
fn websocket_runserver_hook_is_registered() {
	// Arrange / Act
	let registered = inventory::iter::<reinhardt::commands::RunserverHookRegistration>
		.into_iter()
		.any(|registration| registration.type_name == "WebSocketRunserverHook");

	// Assert
	assert!(
		registered,
		"WebSocketRunserverHook must be registered for manage runserver; \
		 see kent8192/reinhardt-cloud#655"
	);
}
