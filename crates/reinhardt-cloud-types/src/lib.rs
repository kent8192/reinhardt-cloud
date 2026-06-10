//! Shared domain types and Kubernetes CRDs for the Reinhardt Cloud PaaS platform.
//!
//! This crate provides domain types and Kubernetes custom resource
//! definitions (CRDs) used across Reinhardt Cloud library crates, the operator,
//! and the reinhardt application layer.

pub mod agent;
pub mod build;
pub mod cluster;
pub mod config;
pub mod crd;
pub mod deployment;
pub mod introspect;
pub mod log;
pub mod reinhardt_cloud_toml;
pub mod user;
pub mod validation;

pub use agent::{AgentCommand, AgentEvent, AgentHealth, AgentInfo};
pub use build::{BuildEvent, BuildPhase, BuildRequest, BuildStatus, EnvVar};
pub use cluster::Cluster;
pub use config::ReinhardtConfig;
pub use crd::{
	ProjectCondition, ConditionStatus, ConditionType, Project, ProjectSpec,
	ProjectStatus,
};
pub use deployment::{Deployment, DeploymentStatus};
pub use log::{LogEntry, LogFilter, LogLevel};
pub use user::User;
pub use validation::ValidationError;
