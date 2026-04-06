//! Plugin system for extending Reinhardt Cloud.
//!
//! Provides traits and registry for gRPC-based plugins that can
//! hook into the build and deployment pipeline.

pub mod registry;
pub mod traits;

pub use registry::PluginRegistry;
pub use traits::{PluginHookType, PluginResult, PluginService};
