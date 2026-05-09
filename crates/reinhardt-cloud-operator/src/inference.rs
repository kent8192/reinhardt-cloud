//! Inference engine for zero-config deployment resource generation.
//!
//! This module provides platform-aware resource inference, automatically
//! generating Kubernetes resources (Secrets, StatefulSets, ConfigMaps for
//! plugins) based on the `ReinhardtApp` spec and the target deployment
//! platform. Application settings are read from the bundled `production.toml`
//! that ships inside each reinhardt-web image; the operator does not emit a
//! per-app settings ConfigMap.

pub(crate) mod database;
pub(crate) mod env_vars;
pub(crate) mod pages;
pub(crate) mod platform;
pub(crate) mod secrets;
