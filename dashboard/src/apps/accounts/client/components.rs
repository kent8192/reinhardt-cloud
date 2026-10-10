//! UI components for the accounts application.
//!
//! Reached only on the WASM target through `#[cfg(client)] pub mod client;`
//! in the parent app aggregator, so contents below do not need additional
//! gates.
//!
//! Route-backed pages (`home`, `sign_in`) live here; `sign_in_preview` is the
//! decorative aside of the sign-in page.

pub mod home;
pub mod sign_in;
pub mod sign_in_preview;
