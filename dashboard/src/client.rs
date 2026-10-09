//! Client-side (WASM) modules for the Control Plane.
//!
//! - `lib` — `#[wasm_bindgen(start)]` entry point (delegates to `ClientLauncher`)
//!
//! UI components live in each application's `client/components/` module.

pub mod lib;
