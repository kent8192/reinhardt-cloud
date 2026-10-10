//! End-to-end tests of GitHub sign-in over real HTTP (SR-01, SR-02, SR-04, SR-06,
//! SR-07, SR-08, SR-19).

use reinhardt::db::orm::Model;
use rstest::rstest;
use serde_json::{Value, json};
use serial_test::serial;

use crate::apps::accounts::models::{SocialAccount, User};
use crate::apps::accounts::tests::server_support::{
	AppOptions, Browser, GithubAccount, TestApp, query_value,
};
use crate::apps::accounts::tests::support::{insert_user, user_count};
use crate::audit::capture::capture_audit_events;

const CURRENT_VIEWER: &str = "/api/server_fn/current_viewer";
const TAKE_NOTICE: &str = "/api/server_fn/take_sign_in_notice";
const SIGN_OUT: &str = "/api/server_fn/sign_out";

async fn viewer(browser: &mut Browser, app: &TestApp) -> Value {
	let reply = browser
		.post_json(CURRENT_VIEWER, json!({}), Some(&app.base_url))
		.await;
	assert_eq!(reply.status, 200, "{reply:?}");
	reply.json()
}

async fn notice(browser: &mut Browser, app: &TestApp) -> Value {
	let reply = browser
		.post_json(TAKE_NOTICE, json!({}), Some(&app.base_url))
		.await;
	assert_eq!(reply.status, 200, "{reply:?}");
	reply.json()
}

async fn user_of(github_user_id: i64) -> Option<User> {
	User::objects()
		.filter(User::field_github_user_id().eq(github_user_id))
		.first()
		.await
		.unwrap()
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_01_github_sign_in_creates_the_user_and_a_hardened_session() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let mut account = GithubAccount::new(5_001, "riley-chen");
	account.name = Some("Riley Chen");
	app.expect_sign_in("code-1", &account).await;
	let mut browser = app.browser();

	// Act
	let (events, callback) = capture_audit_events(browser.sign_in("code-1")).await;

	// Assert
	assert_eq!(callback.status, 302, "{callback:?}");
	assert_eq!(callback.header("location"), Some("/"));
	let session = callback
		.set_cookie("cloud_session")
		.expect("the callback sets the session cookie");
	assert!(session.contains("; HttpOnly"), "{session}");
	assert!(session.contains("; SameSite=Lax"), "{session}");
	assert!(session.contains("; Path=/;"), "{session}");
	assert!(session.contains("; Max-Age=86400"), "{session}");
	assert!(
		!session.contains("Secure"),
		"the ci profile is plain HTTP: {session}"
	);
	let binding = callback
		.set_cookie("cloud_signin_binding")
		.expect("the callback clears the binding cookie");
	assert!(binding.contains("Max-Age=0"), "{binding}");

	let user = user_of(5_001).await.expect("the User exists");
	assert_eq!(user.github_login, "riley-chen");
	assert_eq!(user.display_name, "Riley Chen");
	assert!(user.is_active && !user.is_staff);
	assert!(user.last_login.is_some(), "the sign-in is recorded");
	assert_eq!(
		viewer(&mut browser, &app).await,
		json!({
			"github_login": "riley-chen",
			"display_name": "Riley Chen",
			"avatar_url": "https://avatars.example.test/u/5001",
			"is_staff": false
		})
	);

	let names: Vec<_> = events.iter().filter_map(|e| e.field("event")).collect();
	assert_eq!(
		names,
		["accounts.sign_up.admitted", "accounts.sign_in.succeeded"]
	);
	let succeeded = &events[1];
	assert_eq!(succeeded.field("outcome"), Some("succeeded"));
	assert_eq!(succeeded.field("actor_kind"), Some("user"));
	assert_eq!(succeeded.field("github_user_id"), Some("5001"));
	let rendered = format!("{events:?}");
	for secret in [
		account.access_token.as_str(),
		account.refresh_token.as_str(),
		"code-1",
		session.as_str(),
	] {
		assert!(
			!rendered.contains(secret),
			"audit events must hold no secret"
		);
	}
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_06_the_provider_tokens_are_stored_encrypted() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(5_002, "mika");
	app.expect_sign_in("code-2", &account).await;
	let mut browser = app.browser();

	// Act
	browser.sign_in("code-2").await;

	// Assert
	let rows = SocialAccount::objects().all().all().await.unwrap();
	assert_eq!(rows.len(), 1);
	let row = &rows[0];
	assert!(!row.encrypted_access_token.contains(&account.access_token));
	assert!(!row.encrypted_access_token.is_empty());
	let refresh = row.encrypted_refresh_token.as_deref().unwrap();
	assert!(!refresh.contains(&account.refresh_token));
	assert!(row.refresh_token_expires_at.is_some());
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_04_the_binding_cookie_is_short_lived_script_inaccessible_and_path_scoped() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let mut browser = app.browser();

	// Act
	let start = browser.get("/api/auth/github/").await;

	// Assert
	assert_eq!(start.status, 302);
	assert_eq!(start.header("cache-control"), Some("no-store"));
	let cookie = start
		.set_cookie("cloud_signin_binding")
		.expect("a binding cookie");
	let (pair, attributes) = cookie.split_once("; ").unwrap();
	assert_eq!(pair.len(), "cloud_signin_binding=".len() + 43);
	assert_eq!(
		attributes,
		"HttpOnly; SameSite=Lax; Path=/api/auth/github/callback/; Max-Age=600"
	);
	let location = start.header("location").unwrap();
	assert!(!location.contains(&pair["cloud_signin_binding=".len()..]));
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_04_a_callback_cannot_be_replayed() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(5_003, "replayed");
	app.expect_sign_in("code-3", &account).await;
	let mut browser = app.browser();
	let start = browser.get("/api/auth/github/").await;
	let state = query_value(start.header("location").unwrap(), "state").unwrap();
	let callback_url = format!("/api/auth/github/callback/?code=code-3&state={state}");
	let first = browser.get(&callback_url).await;
	let mut attacker = app.browser();
	attacker.set_cookie("cloud_signin_binding", "anything");

	// Act
	let replay = attacker.get(&callback_url).await;

	// Assert
	assert_eq!(first.header("location"), Some("/"));
	assert_eq!(replay.status, 302);
	assert_eq!(replay.header("location"), Some("/sign-in/"));
	assert!(attacker.cookie("cloud_session").is_none());
	assert_eq!(viewer(&mut attacker, &app).await, Value::Null);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_04_a_callback_from_another_browser_signs_nobody_in_and_uses_the_state_up() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(5_004, "victim-flow");
	app.expect_sign_in("code-4", &account).await;
	let mut victim = app.browser();
	let start = victim.get("/api/auth/github/").await;
	let state = query_value(start.header("location").unwrap(), "state").unwrap();
	let callback_url = format!("/api/auth/github/callback/?code=code-4&state={state}");
	let mut attacker = app.browser();

	// Act: the attacker's browser (no binding cookie, then a forged one) completes
	// the victim's flow; afterwards the victim's own browser tries.
	let without_cookie = attacker.get(&callback_url).await;
	let (events, victim_late) = capture_audit_events(victim.get(&callback_url)).await;

	// Assert
	assert_eq!(without_cookie.header("location"), Some("/sign-in/"));
	assert!(attacker.cookie("cloud_session").is_none());
	assert_eq!(
		victim_late.header("location"),
		Some("/sign-in/"),
		"the rejected attempt consumed the state"
	);
	assert_eq!(user_count().await, 0);
	assert_eq!(events.len(), 1);
	assert_eq!(events[0].field("event"), Some("accounts.sign_in.failed"));
	assert_eq!(events[0].field("reason"), Some("invalid_state"));
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_04_a_declined_authorization_ends_in_the_sign_in_page_and_uses_the_state_up() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(5_005, "declined");
	app.expect_sign_in("code-5", &account).await;
	let mut browser = app.browser();
	let start = browser.get("/api/auth/github/").await;
	let state = query_value(start.header("location").unwrap(), "state").unwrap();

	// Act
	let declined = browser
		.get(&format!(
			"/api/auth/github/callback/?error=access_denied&error_description=nope&state={state}"
		))
		.await;
	let late = browser
		.get(&format!(
			"/api/auth/github/callback/?code=code-5&state={state}"
		))
		.await;

	// Assert
	assert_eq!(declined.header("location"), Some("/sign-in/"));
	assert_eq!(late.header("location"), Some("/sign-in/"));
	assert_eq!(user_count().await, 0);
	assert_eq!(notice(&mut browser, &app).await, json!("Failed"));
}

#[rstest]
#[case::empty("")]
#[case::unknown_state("?code=c&state=never-issued")]
#[case::no_state("?code=c")]
#[case::no_code("?state=never-issued")]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_04_a_malformed_callback_ends_in_the_sign_in_page(#[case] query: &str) {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let mut browser = app.browser();

	// Act
	let reply = browser
		.get(&format!("/api/auth/github/callback/{query}"))
		.await;

	// Assert
	assert_eq!(reply.status, 302);
	assert_eq!(reply.header("location"), Some("/sign-in/"));
	assert!(browser.cookie("cloud_session").is_none());
	assert_eq!(user_count().await, 0);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_08_each_sign_in_rotates_the_session_and_ends_the_one_presented() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(5_006, "rotator");
	app.expect_sign_in("code-6a", &account).await;
	app.expect_sign_in("code-6b", &account).await;
	let mut browser = app.browser();
	browser.sign_in("code-6a").await;
	let first = browser.cookie("cloud_session").unwrap().to_owned();

	// Act
	browser.sign_in("code-6b").await;
	let second = browser.cookie("cloud_session").unwrap().to_owned();
	let mut stale = app.browser();
	stale.set_cookie("cloud_session", &first);

	// Assert
	assert_ne!(first, second);
	assert_eq!(viewer(&mut stale, &app).await, Value::Null);
	assert_eq!(viewer(&mut browser, &app).await["github_login"], "rotator");
	assert_eq!(user_count().await, 1);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_08_a_sign_in_that_cannot_end_the_presented_session_issues_no_session() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(5_016, "stuck-session");
	app.expect_sign_in("code-16a", &account).await;
	app.expect_sign_in("code-16b", &account).await;
	let mut browser = app.browser();
	browser.sign_in("code-16a").await;
	let first = browser.cookie("cloud_session").unwrap().to_owned();
	// Make the delete of the presented session fail: `GETDEL` on a list is a
	// `WRONGTYPE` error and leaves the key in place.
	let client = redis::Client::open(app.redis_url.as_str()).unwrap();
	let mut connection = client.get_multiplexed_async_connection().await.unwrap();
	let keys: Vec<String> = redis::cmd("KEYS")
		.arg("*:s:*")
		.query_async(&mut connection)
		.await
		.unwrap();
	assert_eq!(keys.len(), 1, "one live session: {keys:?}");
	let _: () = redis::cmd("DEL")
		.arg(&keys[0])
		.query_async(&mut connection)
		.await
		.unwrap();
	let _: () = redis::cmd("LPUSH")
		.arg(&keys[0])
		.arg("not-a-session-record")
		.query_async(&mut connection)
		.await
		.unwrap();

	// Act
	let (events, callback) = capture_audit_events(browser.sign_in("code-16b")).await;

	// Assert
	assert_eq!(
		callback.header("location"),
		Some("/sign-in/"),
		"{callback:?}"
	);
	assert!(
		callback.set_cookie("cloud_session").is_none(),
		"no session is issued: {callback:?}"
	);
	assert_eq!(browser.cookie("cloud_session"), Some(first.as_str()));
	let last = events.last().expect("the failure is audited");
	assert_eq!(last.field("event"), Some("accounts.sign_in.failed"));
	assert_eq!(last.field("outcome"), Some("failed"));
	assert!(
		events
			.iter()
			.all(|event| event.field("event") != Some("accounts.sign_in.succeeded"))
	);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_08_a_session_planted_before_sign_in_is_worthless_afterwards() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(5_007, "fixated");
	app.expect_sign_in("code-7", &account).await;
	let mut browser = app.browser();
	browser.set_cookie("cloud_session", "attacker-chosen-session-id");

	// Act
	browser.sign_in("code-7").await;

	// Assert
	assert_ne!(
		browser.cookie("cloud_session"),
		Some("attacker-chosen-session-id")
	);
	let mut attacker = app.browser();
	attacker.set_cookie("cloud_session", "attacker-chosen-session-id");
	assert_eq!(viewer(&mut attacker, &app).await, Value::Null);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_08_sign_out_destroys_the_session_on_the_server() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(5_008, "leaver");
	app.expect_sign_in("code-8", &account).await;
	let mut browser = app.browser();
	browser.sign_in("code-8").await;
	let copied = browser.cookie("cloud_session").unwrap().to_owned();

	// Act
	let (events, out) =
		capture_audit_events(browser.post_json(SIGN_OUT, json!({}), Some(&app.base_url))).await;

	// Assert
	assert_eq!(out.status, 200, "{out:?}");
	let cleared = out
		.set_cookie("cloud_session")
		.expect("the cookie is cleared");
	assert!(cleared.contains("Max-Age=0"), "{cleared}");
	assert!(browser.cookie("cloud_session").is_none());
	let mut thief = app.browser();
	thief.set_cookie("cloud_session", &copied);
	assert_eq!(
		viewer(&mut thief, &app).await,
		Value::Null,
		"the copied cookie stops working because the session is gone, not just forgotten"
	);
	assert_eq!(events.len(), 1);
	assert_eq!(
		events[0].field("event"),
		Some("accounts.sign_out.succeeded")
	);
	assert_eq!(events[0].field("outcome"), Some("succeeded"));
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_07_a_deactivated_user_is_anonymous_on_the_next_request() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(5_009, "soon-gone");
	app.expect_sign_in("code-9", &account).await;
	let mut browser = app.browser();
	browser.sign_in("code-9").await;
	assert_eq!(
		viewer(&mut browser, &app).await["github_login"],
		"soon-gone"
	);

	// Act
	User::objects()
		.filter(User::field_github_user_id().eq(5_009_i64))
		.update_fields([User::field_is_active().assign(false)])
		.await
		.unwrap();

	// Assert
	assert_eq!(viewer(&mut browser, &app).await, Value::Null);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_07_a_deleted_user_is_anonymous_on_the_next_request() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(5_010, "erased");
	app.expect_sign_in("code-10", &account).await;
	let mut browser = app.browser();
	browser.sign_in("code-10").await;
	let user = user_of(5_010).await.unwrap();

	// Act
	for row in SocialAccount::objects().all().all().await.unwrap() {
		SocialAccount::objects().delete(row.id).await.unwrap();
	}
	User::objects().delete(user.id).await.unwrap();

	// Assert
	assert_eq!(viewer(&mut browser, &app).await, Value::Null);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_02_a_renamed_github_login_updates_the_same_user_at_the_next_sign_in() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let before = GithubAccount::new(5_011, "old-name");
	app.expect_sign_in("code-11a", &before).await;
	let mut browser = app.browser();
	browser.sign_in("code-11a").await;
	let original = user_of(5_011).await.unwrap();
	let mut after = GithubAccount::new(5_011, "new-name");
	after.access_token = "ghu_access_after_rename".to_owned();
	after.refresh_token = "ghr_refresh_after_rename".to_owned();
	app.expect_sign_in("code-11b", &after).await;

	// Act
	browser.sign_in("code-11b").await;

	// Assert
	let renamed = user_of(5_011).await.unwrap();
	assert_eq!(renamed.id, original.id);
	assert_eq!(renamed.github_login, "new-name");
	assert_eq!(user_count().await, 1);
	assert_eq!(SocialAccount::objects().all().all().await.unwrap().len(), 1);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_19_invite_only_turns_an_unknown_account_away_and_leaves_no_trace() {
	// Arrange
	let app = TestApp::start(AppOptions {
		sign_up_policy: "invite_only",
		..AppOptions::default()
	})
	.await;
	let account = GithubAccount::new(5_012, "riley-chen");
	app.expect_sign_in("code-12", &account).await;
	let mut browser = app.browser();

	// Act
	let (events, callback) = capture_audit_events(browser.sign_in("code-12")).await;

	// Assert
	assert_eq!(callback.header("location"), Some("/sign-in/"));
	assert!(browser.cookie("cloud_session").is_none());
	assert_eq!(user_count().await, 0);
	assert_eq!(SocialAccount::objects().all().all().await.unwrap().len(), 0);
	let names: Vec<_> = events.iter().filter_map(|e| e.field("event")).collect();
	assert_eq!(
		names,
		["accounts.sign_up.denied", "accounts.sign_in.denied"]
	);
	assert!(events.iter().all(|e| e.field("outcome") == Some("denied")));
	assert_eq!(events[1].field("reason"), Some("not_invited"));
	assert_eq!(events[1].field("github_user_id"), Some("5012"));
	// The page learns the visitor's own login, once, and nothing about the policy.
	assert_eq!(
		notice(&mut browser, &app).await,
		json!({"NotInvited": {"login": "riley-chen"}})
	);
	assert_eq!(notice(&mut browser, &app).await, Value::Null);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_19_a_staff_user_pre_provisioned_signs_in_under_invite_only() {
	// Arrange
	let app = TestApp::start(AppOptions {
		sign_up_policy: "invite_only",
		..AppOptions::default()
	})
	.await;
	let provisioned = insert_user(5_013, "ops", true).await;
	let account = GithubAccount::new(5_013, "ops");
	app.expect_sign_in("code-13", &account).await;
	let mut browser = app.browser();

	// Act
	let callback = browser.sign_in("code-13").await;

	// Assert
	assert_eq!(callback.header("location"), Some("/"));
	assert_eq!(viewer(&mut browser, &app).await["is_staff"], true);
	assert_eq!(user_of(5_013).await.unwrap().id, provisioned.id);
	assert_eq!(user_count().await, 1);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_19_the_allowlist_admits_members_of_a_listed_organization_verified_with_github() {
	// Arrange
	let app = TestApp::start(AppOptions {
		sign_up_policy: "allowlist",
		allowed_organization_ids: "7001",
		..AppOptions::default()
	})
	.await;
	let mut member = GithubAccount::new(5_014, "member");
	member.organization_ids = vec![7_001, 7_002];
	let mut outsider = GithubAccount::new(5_015, "outsider");
	outsider.organization_ids = vec![9_999];
	let mut invitee = GithubAccount::new(5_030, "invitee");
	invitee.pending_organization_ids = vec![7_001];
	app.expect_sign_in("code-14", &member).await;
	app.expect_sign_in("code-15", &outsider).await;
	app.expect_sign_in("code-30", &invitee).await;
	let mut member_browser = app.browser();
	let mut outsider_browser = app.browser();
	let mut invitee_browser = app.browser();

	// Act
	let admitted = member_browser.sign_in("code-14").await;
	let denied = outsider_browser.sign_in("code-15").await;
	let not_yet_a_member = invitee_browser.sign_in("code-30").await;

	// Assert
	assert_eq!(admitted.header("location"), Some("/"));
	assert_eq!(denied.header("location"), Some("/sign-in/"));
	assert_eq!(
		not_yet_a_member.header("location"),
		Some("/sign-in/"),
		"a pending invitation is not membership"
	);
	assert!(user_of(5_014).await.is_some());
	assert!(user_of(5_015).await.is_none());
	assert!(user_of(5_030).await.is_none());
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_19_an_organization_lookup_that_fails_denies_the_sign_up() {
	// Arrange
	let app = TestApp::start(AppOptions {
		sign_up_policy: "allowlist",
		allowed_organization_ids: "7001",
		..AppOptions::default()
	})
	.await;
	let mut account = GithubAccount::new(5_016, "unverifiable");
	account.organization_ids = vec![7_001];
	// The token exchange and profile work; the organization list does not.
	app.expect_sign_in("code-16", &account).await;
	wiremock::Mock::given(wiremock::matchers::method("GET"))
		.and(wiremock::matchers::path("/user/memberships/orgs"))
		.respond_with(wiremock::ResponseTemplate::new(500).set_body_string("PROVIDER-DETAIL"))
		.with_priority(1)
		.mount(&app.github)
		.await;
	let mut browser = app.browser();

	// Act
	let (events, callback) = capture_audit_events(browser.sign_in("code-16")).await;

	// Assert
	assert_eq!(callback.header("location"), Some("/sign-in/"));
	assert!(user_of(5_016).await.is_none());
	let denied = events
		.iter()
		.find(|e| e.field("event") == Some("accounts.sign_in.denied"));
	assert_eq!(
		denied.and_then(|e| e.field("reason")),
		Some("membership_unverified")
	);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_07_a_deactivated_user_cannot_sign_in_and_nothing_is_stored() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let existing = insert_user(5_017, "banned", false).await;
	User::objects()
		.filter(User::field_id().eq(existing.id))
		.update_fields([User::field_is_active().assign(false)])
		.await
		.unwrap();
	let account = GithubAccount::new(5_017, "banned");
	app.expect_sign_in("code-17", &account).await;
	let mut browser = app.browser();

	// Act
	let (events, callback) = capture_audit_events(browser.sign_in("code-17")).await;

	// Assert
	assert_eq!(callback.header("location"), Some("/sign-in/"));
	assert!(browser.cookie("cloud_session").is_none());
	assert_eq!(SocialAccount::objects().all().all().await.unwrap().len(), 0);
	assert!(user_of(5_017).await.unwrap().last_login.is_none());
	let denied = events
		.iter()
		.find(|e| e.field("event") == Some("accounts.sign_in.denied"));
	assert_eq!(denied.and_then(|e| e.field("reason")), Some("deactivated"));
	assert_eq!(
		notice(&mut browser, &app).await,
		json!("Failed"),
		"a deactivated account is not told it lacks an invitation"
	);
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sign_in_without_a_configured_github_app_ends_in_the_sign_in_page() {
	// Arrange
	let app = TestApp::start(AppOptions {
		github_configured: false,
		..AppOptions::default()
	})
	.await;
	let mut browser = app.browser();

	// Act
	let start = browser.get("/api/auth/github/").await;
	let callback = browser
		.get("/api/auth/github/callback/?code=c&state=s")
		.await;

	// Assert
	assert_eq!(start.header("location"), Some("/sign-in/"));
	assert_eq!(callback.header("location"), Some("/sign-in/"));
	assert_eq!(notice(&mut browser, &app).await, json!("Failed"));
}

#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn signing_out_without_a_session_is_not_an_error() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let account = GithubAccount::new(5_018, "twice");
	app.expect_sign_in("code-18", &account).await;
	let mut browser = app.browser();
	browser.sign_in("code-18").await;
	let copied = browser.cookie("cloud_session").unwrap().to_owned();
	browser
		.post_json(SIGN_OUT, json!({}), Some(&app.base_url))
		.await;
	browser.set_cookie("cloud_session", &copied);

	// Act
	let again = browser
		.post_json(SIGN_OUT, json!({}), Some(&app.base_url))
		.await;

	// Assert
	assert_eq!(again.status, 200, "{again:?}");
}
