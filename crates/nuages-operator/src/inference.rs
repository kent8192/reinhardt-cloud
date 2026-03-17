//! Inference engine for zero-config deployment resource generation.
//!
//! This module provides platform-aware resource inference, automatically
//! generating Kubernetes resources (Secrets, ConfigMaps, StatefulSets, etc.)
//! based on the `ReinhardtApp` spec and the target deployment platform.

pub(crate) mod configmap;
pub(crate) mod database;
pub(crate) mod env_vars;
pub(crate) mod platform;
pub(crate) mod secrets;
