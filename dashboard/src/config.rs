//! Configuration module for Reinhardt Cloud

pub mod admin;
pub mod apps;
pub mod grpc;
pub mod grpc_client;
pub mod hooks;
pub mod middleware;
pub mod settings;
pub mod test_helpers;
pub mod urls;

pub use grpc_client::GrpcChannelSingleton;
