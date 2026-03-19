//! Auth application module.
//!
//! Provides JWT-based authentication endpoints for login and registration.

use reinhardt::app_config;

pub mod admin;
pub mod models;
pub mod serializers;
pub mod services;
pub mod tests;
pub mod urls;
pub mod views;

#[app_config(name = "auth", label = "auth")]
pub struct AuthConfig;
