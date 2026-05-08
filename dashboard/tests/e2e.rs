// Native-only — `tests/wasm.rs` covers the wasm32 target. The native
// e2e tests pull in tokio/reqwest/uuid/etc. that don't link for
// `wasm32-unknown-unknown`, so they would break `wasm-pack test` (which
// passes `--tests` to cargo and rebuilds every integration test crate).
// Refs #574.
#![cfg(not(target_arch = "wasm32"))]

#[path = "e2e/auth_clusters.rs"]
mod auth_clusters;
#[path = "e2e/auth_clusters_deployments.rs"]
mod auth_clusters_deployments;
#[path = "e2e/clusters_deployments.rs"]
mod clusters_deployments;
