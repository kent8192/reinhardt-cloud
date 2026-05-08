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
use reinhardt_cloud_dashboard::server;
use rstest::rstest;
use serial_test::serial;

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
