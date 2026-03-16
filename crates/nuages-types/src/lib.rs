//! Shared domain types for the nuages PaaS platform.
//!
//! This crate provides framework-agnostic domain types used across
//! nuages library crates and the reinhardt application layer.

pub mod cluster;
pub mod crd;
pub mod deployment;
pub mod user;

pub use cluster::Cluster;
pub use crd::{ReinhardtApp, ReinhardtAppSpec, ReinhardtAppStatus};
pub use deployment::{Deployment, DeploymentStatus};
pub use user::User;
