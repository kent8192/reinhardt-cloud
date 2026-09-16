// Native-only — see `tests/wasm.rs` for browser tests. Refs #574.
#![cfg(not(target_arch = "wasm32"))]

#[path = "unit/test_routes_configuration.rs"]
mod test_routes_configuration;

#[path = "unit/test_component_style_contract.rs"]
mod test_component_style_contract;
