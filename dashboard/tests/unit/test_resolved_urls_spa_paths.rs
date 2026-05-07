//! Regression coverage for the typed `ResolvedUrls` SPA route accessors.
//!
//! The `#[routes]` macro emits `urls.client().<app>().<route>()` accessors
//! whose path strings come from each app's `#[url_patterns(... mode = client)]`
//! / `#[url_patterns(... mode = unified)]` declarations. These tests assert
//! the rendered paths so that:
//!
//! - regressions in `#[routes]` macro emission or per-app route mounting are
//!   caught at the unit-test layer (no DB / TestContainers required), and
//! - the original coverage from the now-deleted `dashboard/src/client/url.rs`
//!   shim is restored against the typed accessor.

use reinhardt::test::APIClient;
use reinhardt_cloud_dashboard::config::test_helpers::{ResolvedUrls, test_app};
use rstest::rstest;

#[rstest]
#[tokio::test(flavor = "multi_thread")]
async fn auth_login_page_resolves_to_expected_path(test_app: (APIClient, ResolvedUrls)) {
	// Arrange
	let (_client, urls) = test_app;

	// Act
	let path = urls.client().auth().login_page();

	// Assert
	assert_eq!(path, "/login");
}

#[rstest]
#[tokio::test(flavor = "multi_thread")]
async fn auth_register_page_resolves_to_expected_path(test_app: (APIClient, ResolvedUrls)) {
	// Arrange
	let (_client, urls) = test_app;

	// Act
	let path = urls.client().auth().register_page();

	// Assert
	assert_eq!(path, "/register");
}

#[rstest]
#[tokio::test(flavor = "multi_thread")]
async fn dashboard_home_resolves_to_expected_path(test_app: (APIClient, ResolvedUrls)) {
	// Arrange
	let (_client, urls) = test_app;

	// Act
	let path = urls.client().dashboard().home();

	// Assert
	assert_eq!(path, "/");
}

#[rstest]
#[tokio::test(flavor = "multi_thread")]
async fn dashboard_clusters_resolves_to_expected_path(test_app: (APIClient, ResolvedUrls)) {
	// Arrange
	let (_client, urls) = test_app;

	// Act
	let path = urls.client().dashboard().clusters();

	// Assert
	assert_eq!(path, "/clusters");
}

#[rstest]
#[tokio::test(flavor = "multi_thread")]
async fn dashboard_deployments_resolves_to_expected_path(test_app: (APIClient, ResolvedUrls)) {
	// Arrange
	let (_client, urls) = test_app;

	// Act
	let path = urls.client().dashboard().deployments();

	// Assert
	assert_eq!(path, "/deployments");
}
