//! OAuth/OIDC integration with reinhardt-auth's `social` feature.
//!
//! This aggregator collects the dashboard-side glue for the framework's
//! social-auth machinery: a database-backed `SocialAccountStorage` impl,
//! and (in later phases) provider configuration, error mapping, and
//! account-linking semantics.
//!
//! No provider conversion in this aggregator (kent8192/reinhardt-cloud#599):
//! this file is a `pub mod` aggregator with no logic of its own. The
//! DI-backed surfaces live in the submodules themselves
//! (`config::OAuthSettings`, `backend::OAuthBackendBox`); the
//! aggregator merely re-exports.

#[cfg(not(target_arch = "wasm32"))]
pub mod backend;
#[cfg(not(target_arch = "wasm32"))]
pub mod config;
#[cfg(not(target_arch = "wasm32"))]
pub mod linking;
#[cfg(not(target_arch = "wasm32"))]
pub mod storage;
#[cfg(not(target_arch = "wasm32"))]
pub mod token_crypto;

#[cfg(not(target_arch = "wasm32"))]
pub use backend::{OAuthBackendBox, OAuthBackendBoxKey};
#[cfg(not(target_arch = "wasm32"))]
pub use config::{OAuthSettings, OAuthSettingsKey, ProviderCredentials};
