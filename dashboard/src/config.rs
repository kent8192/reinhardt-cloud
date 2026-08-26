//! Configuration module for Reinhardt Cloud

#[cfg(native)]
pub mod admin;
pub mod apps;
#[cfg(native)]
pub mod grpc;
#[cfg(native)]
pub mod grpc_client;
#[cfg(native)]
pub mod hooks;
#[cfg(native)]
pub mod management;
#[cfg(native)]
pub mod middleware;
#[cfg(native)]
pub mod settings;
#[cfg(native)]
pub mod test_helpers;
pub mod urls;

#[cfg(native)]
pub use grpc::{AgentRegistrySingleton, AgentRegistrySingletonKey};
#[cfg(native)]
pub use grpc_client::{GrpcChannelSingleton, GrpcChannelSingletonKey};
#[cfg(native)]
pub use settings::{ProjectSettings, ProjectSettingsKey};
