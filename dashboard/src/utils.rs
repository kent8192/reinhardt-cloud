//! Utility modules for Reinhardt Cloud.
//!
//! Infrastructure utilities that don't follow the Django-style app structure.

#[cfg(native)]
pub(crate) mod grpc;
#[cfg(native)]
pub mod realtime;
#[cfg(native)]
pub mod vcs;
