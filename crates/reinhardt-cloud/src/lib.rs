//! Facade crate for Reinhardt Cloud library components.
//!
//! This crate provides a single dependency that re-exports the public
//! library crates in this workspace under stable module-style names.
//! Binary crates such as the CLI, operator, and cluster agent are not
//! re-exported because they do not expose library APIs.

pub use reinhardt_cloud_core as core;
pub use reinhardt_cloud_grpc as grpc;
pub use reinhardt_cloud_k8s as k8s;
pub use reinhardt_cloud_proto as proto;
pub use reinhardt_cloud_telemetry as telemetry;
pub use reinhardt_cloud_types as types;

/// Convenient access to the facade's component namespaces.
pub mod prelude {
	pub use crate::{core, grpc, k8s, proto, telemetry, types};
}
