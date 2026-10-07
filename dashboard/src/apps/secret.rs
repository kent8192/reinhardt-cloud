//! Secret responsibility boundary for the Control Plane.

#[cfg(server)]
use reinhardt::app_config;

#[cfg(server)]
#[app_config(name = "secret", label = "secret")]
pub struct SecretConfig;

pub mod urls;

#[cfg(server)]
pub mod models;
pub mod services;
