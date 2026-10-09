//! Client-side (WASM) modules for the logs application.
//!
//! Reached only on the WASM target via `#[cfg(client)] pub mod client;`
//! in the parent app aggregator; contents below need no additional gates.
//!
//! Add route-backed components under `components/` and register them from
//! `../urls/client_router.rs`. Add component-scoped styles in `style.rs`.

pub mod components;
pub mod hooks;
pub mod style;
