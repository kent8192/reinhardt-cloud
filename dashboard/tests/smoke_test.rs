//! End-to-end smoke tests for the reinhardt-cloud-dashboard application.

// Native-only — see `tests/wasm.rs` for browser tests. Refs #574.
#![cfg(not(target_arch = "wasm32"))]

use rstest::rstest;

#[rstest]
fn test_installed_apps_not_empty() {
	// Arrange & Act
	let apps = reinhardt_cloud_dashboard::config::apps::get_installed_apps();

	// Assert
	assert!(!apps.is_empty());
	assert!(apps.contains(&"auth".to_string()));
	assert!(apps.contains(&"clusters".to_string()));
	assert!(apps.contains(&"deployments".to_string()));
}
