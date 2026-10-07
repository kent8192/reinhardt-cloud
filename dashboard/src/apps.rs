//! Application registry for cloud_dashboard
//!
//! This file maintains the list of installed apps.
//! New apps created with `startapp` will be automatically added here.
pub mod identity;
#[cfg(server)]
pub use identity::IdentityConfig;
pub mod organization;
#[cfg(server)]
pub use organization::OrganizationConfig;
pub mod project;
#[cfg(server)]
pub use project::ProjectConfig;
pub mod source;
#[cfg(server)]
pub use source::SourceConfig;
pub mod deployment;
#[cfg(server)]
pub use deployment::DeploymentConfig;
pub mod cluster;
#[cfg(server)]
pub use cluster::ClusterConfig;
pub mod secret;
#[cfg(server)]
pub use secret::SecretConfig;
pub mod observability;
#[cfg(server)]
pub use observability::ObservabilityConfig;
