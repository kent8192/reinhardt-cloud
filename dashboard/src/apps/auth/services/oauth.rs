//! OAuth/OIDC integration with reinhardt-auth's `social` feature.
//!
//! This aggregator collects the dashboard-side glue for the framework's
//! social-auth machinery: a database-backed `SocialAccountStorage` impl,
//! and (in later phases) provider configuration, error mapping, and
//! account-linking semantics.

#[cfg(not(target_arch = "wasm32"))]
pub mod storage;
