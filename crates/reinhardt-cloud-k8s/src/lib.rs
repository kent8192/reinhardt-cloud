//! Kubernetes client library for the Reinhardt Cloud PaaS platform.
//!
//! Provides a thin wrapper around kube-rs for cluster management operations.

pub mod client;
pub mod resources;

pub use client::{K8sError, KubeClient};
