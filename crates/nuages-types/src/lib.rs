//! Shared domain types and Kubernetes CRDs for the nuages PaaS platform.
//!
//! This crate provides domain types and Kubernetes custom resource
//! definitions (CRDs) used across nuages library crates, the operator,
//! and the reinhardt application layer.

pub mod cluster;
pub mod config;
pub mod crd;
pub mod deployment;
pub mod user;
pub mod validation;

pub use cluster::Cluster;
pub use config::ReinhardtConfig;
pub use crd::{AppCondition, ReinhardtApp, ReinhardtAppSpec, ReinhardtAppStatus};
pub use deployment::{Deployment, DeploymentStatus};
pub use user::User;
pub use validation::ValidationError;
