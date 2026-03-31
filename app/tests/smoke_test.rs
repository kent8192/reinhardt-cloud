//! End-to-end smoke tests for the reinhardt-cloud application.

use rstest::rstest;

#[rstest]
fn test_installed_apps_not_empty() {
	// Arrange & Act
	let apps = reinhardt_cloud::config::apps::get_installed_apps();

	// Assert
	assert!(!apps.is_empty());
	assert!(apps.contains(&"auth".to_string()));
	assert!(apps.contains(&"clusters".to_string()));
	assert!(apps.contains(&"deployments".to_string()));
}
