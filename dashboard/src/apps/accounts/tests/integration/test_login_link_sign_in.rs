//! Confirming a Login Link over real HTTP: the session it creates, single use,
//! GET-safety, and the cross-site rule (SR-12, SR-16, SR-17, SR-18, SR-20).

use chrono::{Duration as ChronoDuration, Utc};
use reinhardt::db::orm::Model;
use rstest::rstest;
use serde_json::{Value, json};
use serial_test::serial;

use crate::apps::accounts::models::User;
use crate::apps::accounts::services::server::login_links::{
	DEFAULT_LIFETIME, LoginLinkSecret, issue, testing,
};
use crate::apps::accounts::tests::server_support::{AppOptions, Browser, Reply, TestApp};
use crate::apps::accounts::tests::support::insert_user;
use crate::audit::capture::capture_audit_events;

const CONSUME: &str = "/api/server_fn/consume_login_link";
const CURRENT_VIEWER: &str = "/api/server_fn/current_viewer";

/// The Control Plane without GitHub sign-in: only Login Links can sign in.
async fn app_without_github() -> TestApp {
	TestApp::start(AppOptions {
		github_configured: false,
		sign_up_policy: "invite_only",
		..AppOptions::default()
	})
	.await
}

async fn confirm(browser: &mut Browser, app: &TestApp, secret: &LoginLinkSecret) -> Reply {
	browser
		.post_json(
			CONSUME,
			json!({"token": secret.expose()}),
			Some(&app.base_url),
		)
		.await
}

async fn viewer(browser: &mut Browser, app: &TestApp) -> Value {
	browser
		.post_json(CURRENT_VIEWER, json!({}), Some(&app.base_url))
		.await
		.json()
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_16_confirming_a_link_signs_the_browser_in_with_a_hardened_session() {
	// Arrange
	let app = app_without_github().await;
	let user = insert_user(8_001, "ops", true).await;
	let link = issue(8_001, DEFAULT_LIFETIME).await.unwrap();
	let mut browser = app.browser();

	// Act
	let (events, reply) = capture_audit_events(confirm(&mut browser, &app, &link.secret)).await;

	// Assert
	assert_eq!(reply.status, 200, "{reply:?}");
	assert_eq!(reply.json(), json!("SignedIn"));
	let session = reply
		.set_cookie("cloud_session")
		.expect("confirming sets the session cookie");
	assert!(session.contains("; HttpOnly"), "{session}");
	assert!(session.contains("; SameSite=Lax"), "{session}");
	assert!(session.contains("; Path=/;"), "{session}");
	assert!(session.contains("; Max-Age=86400"), "{session}");
	assert_eq!(
		viewer(&mut browser, &app).await,
		json!({
			"github_login": "ops",
			"display_name": "ops",
			"avatar_url": null,
			"is_staff": true
		})
	);
	let after = User::objects()
		.filter(User::field_id().eq(user.id))
		.first()
		.await
		.unwrap()
		.unwrap();
	assert!(after.last_login.is_some(), "the sign-in is recorded");
	let names: Vec<_> = events.iter().filter_map(|e| e.field("event")).collect();
	assert_eq!(names, ["accounts.login_link.consumed"]);
	assert!(!format!("{events:?}").contains(link.secret.expose()));
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_16_a_second_confirmation_of_the_same_link_signs_nobody_in() {
	// Arrange
	let app = app_without_github().await;
	insert_user(8_002, "ops", true).await;
	let link = issue(8_002, DEFAULT_LIFETIME).await.unwrap();
	let mut first = app.browser();
	let mut second = app.browser();
	confirm(&mut first, &app, &link.secret).await;

	// Act
	let replay = confirm(&mut second, &app, &link.secret).await;

	// Assert
	assert_eq!(replay.status, 200, "{replay:?}");
	assert_eq!(replay.json(), json!("Rejected"));
	assert!(replay.set_cookie("cloud_session").is_none());
	assert_eq!(viewer(&mut second, &app).await, json!(null));
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_16_two_browsers_racing_for_one_link_leave_exactly_one_signed_in() {
	// Arrange
	let app = app_without_github().await;
	insert_user(8_003, "ops", true).await;
	let link = issue(8_003, DEFAULT_LIFETIME).await.unwrap();
	let mut left = app.browser();
	let mut right = app.browser();

	// Act
	let (a, b) = tokio::join!(
		confirm(&mut left, &app, &link.secret),
		confirm(&mut right, &app, &link.secret)
	);

	// Assert
	let mut answers = [a.json(), b.json()];
	answers.sort_by_key(Value::to_string);
	assert_eq!(answers, [json!("Rejected"), json!("SignedIn")]);
	let signed_in = [
		viewer(&mut left, &app).await,
		viewer(&mut right, &app).await,
	]
	.iter()
	.filter(|viewer| !viewer.is_null())
	.count();
	assert_eq!(signed_in, 1);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_16_opening_the_url_or_sending_a_get_consumes_nothing() {
	// Arrange
	let app = app_without_github().await;
	insert_user(8_004, "ops", true).await;
	let link = issue(8_004, DEFAULT_LIFETIME).await.unwrap();
	let mut previewer = app.browser();
	let page = format!("/sign-in/link/#{}", link.secret.expose());
	let with_query = format!("{CONSUME}?token={}", link.secret.expose());

	// Act
	let page_reply = previewer.get(&page).await;
	let get_reply = previewer.get(&with_query).await;
	let head_like = previewer
		.get_with(&with_query, &[("Origin", &app.base_url)])
		.await;

	// Assert
	// The page request never carried the fragment, so the server could not have
	// seen the secret; the confirmation endpoint refuses to act on a GET.
	assert!(
		!page_reply.body.contains(link.secret.expose()),
		"the served page does not echo the secret"
	);
	assert!(
		![200, 302].contains(&get_reply.status) && ![200, 302].contains(&head_like.status),
		"a GET must not confirm: {get_reply:?} {head_like:?}"
	);
	assert!(get_reply.set_cookie("cloud_session").is_none());
	let mut human = app.browser();
	let confirmed = confirm(&mut human, &app, &link.secret).await;
	assert_eq!(
		confirmed.json(),
		json!("SignedIn"),
		"the link is still unused"
	);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_16_every_kind_of_bad_link_gets_the_identical_response() {
	// Arrange
	let app = app_without_github().await;
	let user = insert_user(8_005, "ops", true).await;
	let inactive = insert_user(8_006, "retired", false).await;
	User::objects()
		.filter(User::field_id().eq(inactive.id))
		.update_fields([User::field_is_active().assign(false)])
		.await
		.unwrap();
	let used = testing::insert(
		user.id,
		Utc::now() + ChronoDuration::minutes(5),
		Some(Utc::now()),
	)
	.await;
	let expired = testing::insert(user.id, Utc::now() - ChronoDuration::seconds(5), None).await;
	let of_inactive =
		testing::insert(inactive.id, Utc::now() + ChronoDuration::minutes(5), None).await;
	let unknown = LoginLinkSecret::from_input("never-issued");
	let empty = LoginLinkSecret::from_input("");
	let mut seen = Vec::new();

	// Act
	for secret in [used, expired, of_inactive, unknown, empty] {
		let mut browser = app.browser();
		let reply = confirm(&mut browser, &app, &secret).await;
		seen.push((
			reply.status,
			reply.body.clone(),
			reply.set_cookie("cloud_session").is_some(),
			reply.header("content-type").map(str::to_owned),
		));
	}

	// Assert
	assert_eq!(seen[0].0, 200);
	assert_eq!(seen[0].1, json!("Rejected").to_string());
	assert!(!seen[0].2);
	assert!(
		seen.iter().all(|reply| reply == &seen[0]),
		"used, expired, deactivated, unknown, and empty links answer alike: {seen:?}"
	);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_12_confirming_a_link_must_come_from_the_dashboard_origin_even_without_a_session() {
	// Arrange
	let app = app_without_github().await;
	insert_user(8_007, "ops", true).await;
	let link = issue(8_007, DEFAULT_LIFETIME).await.unwrap();
	let body = json!({"token": link.secret.expose()});
	let mut browser = app.browser();

	// Act
	let foreign = browser
		.post_json(CONSUME, body.clone(), Some("https://evil.example"))
		.await;
	let none = browser.post_json(CONSUME, body.clone(), None).await;
	let right = browser.post_json(CONSUME, body, Some(&app.base_url)).await;

	// Assert
	assert_eq!(foreign.status, 403, "{foreign:?}");
	assert_eq!(none.status, 403, "{none:?}");
	assert!(foreign.set_cookie("cloud_session").is_none());
	assert_eq!(
		right.json(),
		json!("SignedIn"),
		"the rejected attempts did not consume the link"
	);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_12_a_foreign_origin_cannot_replace_an_existing_session_with_a_link() {
	// Arrange
	let app = app_without_github().await;
	insert_user(8_008, "victim", false).await;
	insert_user(8_009, "attacker", false).await;
	let victim_link = issue(8_008, DEFAULT_LIFETIME).await.unwrap();
	let attacker_link = issue(8_009, DEFAULT_LIFETIME).await.unwrap();
	let mut browser = app.browser();
	confirm(&mut browser, &app, &victim_link.secret).await;

	// Act
	let forged = browser
		.post_json(
			CONSUME,
			json!({"token": attacker_link.secret.expose()}),
			Some("https://evil.example"),
		)
		.await;

	// Assert
	assert_eq!(forged.status, 403, "{forged:?}");
	assert_eq!(
		viewer(&mut browser, &app).await["github_login"],
		json!("victim")
	);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_08_confirming_a_link_rotates_the_session_the_browser_presented() {
	// Arrange
	let app = app_without_github().await;
	insert_user(8_010, "first", false).await;
	insert_user(8_011, "second", false).await;
	let first_link = issue(8_010, DEFAULT_LIFETIME).await.unwrap();
	let second_link = issue(8_011, DEFAULT_LIFETIME).await.unwrap();
	let mut browser = app.browser();
	confirm(&mut browser, &app, &first_link.secret).await;
	let old_token = browser.cookie("cloud_session").unwrap().to_owned();

	// Act
	let reply = confirm(&mut browser, &app, &second_link.secret).await;

	// Assert
	assert_eq!(reply.json(), json!("SignedIn"));
	let new_token = browser.cookie("cloud_session").unwrap().to_owned();
	assert_ne!(old_token, new_token);
	assert_eq!(
		viewer(&mut browser, &app).await["github_login"],
		json!("second")
	);
	let mut replay = app.browser();
	replay.set_cookie("cloud_session", &old_token);
	assert_eq!(
		viewer(&mut replay, &app).await,
		json!(null),
		"the session presented at sign-in no longer works"
	);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_07_a_link_session_is_revalidated_against_the_user_on_every_request() {
	// Arrange
	let app = app_without_github().await;
	let user = insert_user(8_012, "ops", true).await;
	let link = issue(8_012, DEFAULT_LIFETIME).await.unwrap();
	let mut browser = app.browser();
	confirm(&mut browser, &app, &link.secret).await;
	assert_eq!(viewer(&mut browser, &app).await["is_staff"], json!(true));

	// Act
	User::objects()
		.filter(User::field_id().eq(user.id))
		.update_fields([User::field_is_staff().assign(false)])
		.await
		.unwrap();
	let demoted = viewer(&mut browser, &app).await;
	User::objects()
		.filter(User::field_id().eq(user.id))
		.update_fields([User::field_is_active().assign(false)])
		.await
		.unwrap();
	let deactivated = viewer(&mut browser, &app).await;

	// Assert
	assert_eq!(demoted["is_staff"], json!(false), "Staff is read fresh");
	assert_eq!(deactivated, json!(null), "a deactivated User is anonymous");
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_18_a_link_signs_in_without_any_github_app() {
	// Arrange: GitHub sign-in is explicitly disabled for this deployment.
	let app = app_without_github().await;
	insert_user(8_013, "break-glass", true).await;
	let link = issue(8_013, DEFAULT_LIFETIME).await.unwrap();
	let mut browser = app.browser();

	// Act
	let github = browser.get("/api/auth/github/").await;
	let confirmed = confirm(&mut browser, &app, &link.secret).await;

	// Assert
	assert_eq!(
		github.header("location"),
		Some("/sign-in/"),
		"no GitHub flow"
	);
	assert_eq!(confirmed.json(), json!("SignedIn"));
	assert_eq!(viewer(&mut browser, &app).await["is_staff"], json!(true));
}
