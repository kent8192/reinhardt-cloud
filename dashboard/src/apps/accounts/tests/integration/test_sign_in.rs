//! End-to-end tests of GitHub sign-in over real HTTP.

use reinhardt::db::orm::Model;
use rstest::rstest;
use serial_test::serial;

use crate::apps::accounts::models::User;
use crate::apps::accounts::tests::server_support::{AppOptions, GithubAccount, TestApp};

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sign_in_creates_a_user_and_a_session_when_the_policy_admits() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(5_001, "riley-chen");
	app.expect_sign_in("code-1", &account).await;
	let mut browser = app.browser();

	// Act
	let callback = browser.sign_in("code-1").await;

	// Assert
	assert_eq!(callback.status, 302, "{callback:?}");
	assert_eq!(callback.header("location"), Some("/"));
	assert!(browser.cookie("cloud_session").is_some());
	let user = User::objects()
		.filter(User::field_github_user_id().eq(5_001_i64))
		.first()
		.await
		.unwrap()
		.expect("the User exists");
	assert_eq!(user.github_login, "riley-chen");
	let viewer = browser
		.post_json(
			"/api/server_fn/current_viewer",
			serde_json::json!({}),
			Some(&app.base_url),
		)
		.await;
	assert_eq!(viewer.status, 200, "{viewer:?}");
	assert_eq!(viewer.body, "x");
}
