//! Unit tests for application routing configuration.

use rstest::rstest;

use reinhardt_cloud_dashboard::config::apps::get_installed_apps;

#[rstest]
fn test_installed_apps_exact_content() {
	// Arrange & Act
	let apps = get_installed_apps();

	// Assert — verify exact app list, not just contains
	assert_eq!(
		apps.len(),
		4,
		"Expected exactly 4 installed apps, got: {:?}",
		apps
	);
	assert!(apps.contains(&"auth".to_string()));
	assert!(apps.contains(&"clusters".to_string()));
	assert!(apps.contains(&"deployments".to_string()));
	assert!(apps.contains(&"health".to_string()));
}
