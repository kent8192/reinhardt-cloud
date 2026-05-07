//! View functions for the dashboard app.
//!
//! Re-exports the shared SPA shell and 404 fallback owned by
//! `crate::client`. Per-section views (clusters/deployments) currently
//! resolve to the 404 placeholder until their dedicated pages land.

pub use crate::client::layout::dashboard_shell;
pub use crate::client::pages::not_found_page;
