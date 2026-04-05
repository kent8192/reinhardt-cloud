//! Protocol buffer definitions and generated gRPC service stubs for Reinhardt Cloud.
//!
//! This crate contains `.proto` files compiled by `tonic-build` at build time,
//! providing both server and client stubs for the build, cluster agent, and log
//! gRPC services.

/// Common protobuf types (pagination, status).
pub mod common {
	tonic::include_proto!("reinhardt.cloud.common");
}

/// Build service protobuf types and gRPC stubs.
pub mod build {
	tonic::include_proto!("reinhardt.cloud.build");
}

/// Cluster agent service protobuf types and gRPC stubs.
pub mod cluster_agent {
	tonic::include_proto!("reinhardt.cloud.cluster_agent");
}

/// Log service protobuf types and gRPC stubs.
pub mod log {
	tonic::include_proto!("reinhardt.cloud.log");
}
