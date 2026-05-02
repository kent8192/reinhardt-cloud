//! Type-safe SPA route accessor layer.
//!
//! Provides compile-time-checked free functions that resolve each named
//! SPA route to its URL path string. Import the submodule for the app
//! whose routes you need:
//!
//! ```rust,ignore
//! use crate::client::client_urls;
//!
//! let url = client_urls::auth::login_page();
//! assert_eq!(url, "/login");
//! ```
//!
//! Path constants in each submodule mirror the route registrations in
//! `config/urls.rs` and `apps/<app>/urls.rs`. The integration test in
//! `client/url.rs` (`native_resolver_delegates_to_global_reverser`)
//! guards against drift between these constants and the live reverser.

pub mod auth;
pub mod dashboard;
