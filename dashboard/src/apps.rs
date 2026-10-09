//! Application registry for cloud_control_plane
//!
//! This file maintains the list of installed apps.
//! New apps created with `startapp` will be automatically added here.
pub mod accounts;
#[cfg(server)]
pub use accounts::AccountsConfig;
pub mod organizations;
#[cfg(server)]
pub use organizations::OrganizationsConfig;
pub mod clusters;
#[cfg(server)]
pub use clusters::ClustersConfig;
pub mod agents;
#[cfg(server)]
pub use agents::AgentsConfig;
pub mod projects;
#[cfg(server)]
pub use projects::ProjectsConfig;
pub mod deployments;
#[cfg(server)]
pub use deployments::DeploymentsConfig;
pub mod logs;
#[cfg(server)]
pub use logs::LogsConfig;
pub mod github;
#[cfg(server)]
pub use github::GithubConfig;
pub mod health;
#[cfg(server)]
pub use health::HealthConfig;
