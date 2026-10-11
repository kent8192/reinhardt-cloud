//! UI components for the accounts application.
//!
//! Reached only on the WASM target through `#[cfg(client)] pub mod client;`
//! in the parent app aggregator, so contents below do not need additional
//! gates.
//!
//! Route-backed pages (`account`, `home`, `login_link`, `sign_in`) live here;
//! `sign_in_preview` is the decorative aside of the sign-in page. `shell` wraps
//! the signed-in pages in the application frame, with the `user_menu` and the
//! `sign_out_button` it offers.

pub mod account;
pub mod home;
pub mod login_link;
pub mod shell;
pub mod sign_in;
pub mod sign_in_preview;
pub mod sign_out_button;
pub mod user_menu;
