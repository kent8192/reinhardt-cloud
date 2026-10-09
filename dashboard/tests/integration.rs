//! Integration tests for the Control Plane application skeleton.

use cloud_control_plane::config::apps::InstalledApp;
use rstest::rstest;

/// The nine applications named by the rebuild plan, in registration order.
const EXPECTED_APPS: [&str; 9] = [
	"accounts",
	"organizations",
	"clusters",
	"agents",
	"projects",
	"deployments",
	"logs",
	"github",
	"health",
];

#[rstest]
fn installed_apps_match_the_application_boundaries() {
	// Arrange
	let expected = EXPECTED_APPS;

	// Act
	let labels = InstalledApp::all_labels();

	// Assert
	assert_eq!(labels, expected.as_slice());
}
