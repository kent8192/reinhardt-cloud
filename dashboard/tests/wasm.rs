//! WASM-target browser tests for the dashboard SPA client.
//!
//! Driven by `wasm-pack test --headless --chrome --features wasm-spa-test`
//! (see `dashboard/Makefile.toml` task `wasm-spa-test`). These tests
//! exercise the SPA navigation chain that ships in production WASM
//! bundles, mirroring the topology used by `dashboard/src/client.rs`.
//!
//! Each test file lives under `tests/wasm/` per `dashboard/CLAUDE.md`
//! TD-3, with the file name describing the feature under test.
//!
//! Refs `kent8192/reinhardt-cloud#574` and upstream
//! `kent8192/reinhardt-web#4221` (7th SPA navigation regression).

#![cfg(all(target_arch = "wasm32", feature = "wasm-spa-test"))]

// `tests/wasm.rs` is the integration-test crate root; its child modules
// live under `tests/wasm/`. Cargo does not auto-discover that layout for
// integration tests (it would expect siblings), so we point at each
// module file explicitly. This keeps the directory structure aligned
// with `dashboard/CLAUDE.md` TD-3 without relying on `mod.rs`.
#[path = "wasm/test_spa_navigation_smoke.rs"]
mod test_spa_navigation_smoke;

#[path = "wasm/test_spa_route_paths.rs"]
mod test_spa_route_paths;

#[path = "wasm/test_frontend_msw_e2e.rs"]
mod test_frontend_msw_e2e;
