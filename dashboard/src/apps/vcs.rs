//! VCS (Version Control System) application module.
//!
//! Provides webhook receiver, signature verification, and event parsing
//! for GitHub and GitLab integrations.

use reinhardt::app_config;

pub mod events;
pub mod signature;

#[app_config(name = "vcs", label = "vcs")]
pub struct VcsConfig;
