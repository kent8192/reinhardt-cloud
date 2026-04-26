//! Application configuration for Reinhardt Cloud
//!
//! This module defines the installed applications using compile-time validation.

use reinhardt::installed_apps;

// Register reinhardt-cloud Django-style apps for discovery and configuration.
// Framework features (auth, sessions, etc.) are enabled via Cargo feature flags.
installed_apps! {
	auth: "auth",
	clusters: "clusters",
	deployments: "deployments",
	health: "health",
	organizations: "organizations",
}

/// Get the list of installed applications
pub fn get_installed_apps() -> Vec<String> {
	InstalledApp::all_apps()
}
