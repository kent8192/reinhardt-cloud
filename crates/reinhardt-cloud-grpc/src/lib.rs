//! gRPC server infrastructure for Reinhardt Cloud.
//!
//! Provides gRPC server configuration, interceptors, health checking,
//! and service registration for the Reinhardt Cloud platform.

// `tonic::Status` is inherently large; boxing it would break the tonic API contract.
#![allow(clippy::result_large_err)]

pub mod config;
pub mod health;
pub mod interceptor;
pub mod registry;
pub mod services;
pub mod settings;
pub mod sse;
pub mod test_utils;
