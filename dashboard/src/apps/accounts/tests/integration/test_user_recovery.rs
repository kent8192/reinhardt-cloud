//! Host-operator recovery: ending a User's sessions and reactivating them
//! (SR-20, SR-107).

use reinhardt::db::orm::Model;
use rstest::rstest;
use serde_json::json;
use serial_test::serial;

use crate::apps::accounts::models::User;
use crate::apps::accounts::services::server::repoint::repoint;
use crate::apps::accounts::services::server::user_recovery::{
	ReactivateOutcome, RecoveryError, end_sessions, reactivate,
};
use crate::apps::accounts::services::server::users::find_by_github_user_id;
use crate::apps::accounts::tests::server_support::{AppOptions, GithubAccount, TestApp};
use crate::apps::accounts::tests::support::{
	ScriptedRevoker, TestDatabase, database, insert_user, redis_sessions,
};
use crate::audit::capture::capture_audit_events;

async fn deactivate(user: &User) {
	User::objects()
		.filter(User::field_id().eq(user.id))
		.update_fields([User::field_is_active().assign(false)])
		.await
		.unwrap();
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_end_sessions_ends_every_session_of_the_user_and_only_theirs(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;
	let user = insert_user(2_001, "target", false).await;
	let other = insert_user(2_002, "bystander", false).await;
	let mine = [
		redis.sessions.create(user.id).await.unwrap(),
		redis.sessions.create(user.id).await.unwrap(),
	];
	let theirs = redis.sessions.create(other.id).await.unwrap();

	// Act
	let (events, ended) = capture_audit_events(end_sessions(2_001, &redis.sessions)).await;

	// Assert
	assert_eq!(ended.unwrap(), 2);
	for session in &mine {
		assert_eq!(redis.sessions.resolve(&session.token).await.unwrap(), None);
	}
	assert_eq!(
		redis.sessions.resolve(&theirs.token).await.unwrap(),
		Some(other.id)
	);
	assert_eq!(events.len(), 1);
	assert_eq!(
		events[0].field("event"),
		Some("accounts.end_sessions.succeeded")
	);
	assert_eq!(events[0].field("actor_kind"), Some("host_operator"));
	assert_eq!(events[0].field("github_user_id"), Some("2001"));
	assert_eq!(events[0].field("outcome"), Some("succeeded"));
	assert_eq!(
		events[0].field("subject_user_id"),
		Some(user.id.to_string().as_str())
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_end_sessions_fails_and_says_so_when_redis_fails(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	insert_user(2_003, "target", false).await;
	let revoker = ScriptedRevoker::failing_after(0);

	// Act
	let (events, result) = capture_audit_events(end_sessions(2_003, &revoker)).await;

	// Assert
	assert!(matches!(result, Err(RecoveryError::SessionsNotEnded(_))));
	assert_eq!(events.len(), 1);
	assert_eq!(
		events[0].field("event"),
		Some("accounts.end_sessions.failed")
	);
	assert_eq!(events[0].field("outcome"), Some("failed"));
	assert_eq!(events[0].field("reason"), Some("sessions_not_ended"));
}

#[rstest]
#[case::unknown_user(2_999, "unknown_user")]
#[case::non_positive(0, "invalid_github_user_id")]
#[tokio::test]
#[serial(database)]
async fn sr_107_end_sessions_denies_an_invalid_or_unknown_id(
	#[future] database: TestDatabase,
	#[case] github_user_id: i64,
	#[case] reason: &'static str,
) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;

	// Act
	let (events, result) =
		capture_audit_events(end_sessions(github_user_id, &redis.sessions)).await;

	// Assert
	assert!(result.is_err());
	assert_eq!(
		events[0].field("event"),
		Some("accounts.end_sessions.denied")
	);
	assert_eq!(events[0].field("reason"), Some(reason));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_reactivation_ends_leftover_sessions_before_the_user_is_active_again(
	#[future] database: TestDatabase,
) {
	// Arrange: a User deactivated by the fallback, with a session nobody presented.
	let _db = database.await;
	let redis = redis_sessions().await;
	let user = insert_user(2_010, "deactivated", true).await;
	let leftover = redis.sessions.create(user.id).await.unwrap();
	deactivate(&user).await;

	// Act
	let (events, outcome) = capture_audit_events(reactivate(2_010, &redis.sessions)).await;

	// Assert
	assert_eq!(
		outcome.unwrap(),
		ReactivateOutcome::Reactivated {
			user_id: user.id,
			sessions_ended: 1
		}
	);
	let after = find_by_github_user_id(2_010).await.unwrap().unwrap();
	assert!(after.is_active && after.is_staff);
	assert_eq!(
		redis.sessions.resolve(&leftover.token).await.unwrap(),
		None,
		"the old session does not come back to life"
	);
	assert_eq!(events.len(), 1);
	assert_eq!(
		events[0].field("event"),
		Some("accounts.reactivate.succeeded")
	);
	assert_eq!(events[0].field("reason"), Some("reactivated"));
	assert_eq!(events[0].field("github_user_id"), Some("2010"));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_reactivation_refuses_when_the_sessions_cannot_be_ended(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(2_011, "deactivated", true).await;
	deactivate(&user).await;
	let revoker = ScriptedRevoker::failing_after(0);

	// Act
	let (events, outcome) = capture_audit_events(reactivate(2_011, &revoker)).await;

	// Assert
	assert!(matches!(outcome, Err(RecoveryError::SessionsNotEnded(_))));
	assert!(
		!find_by_github_user_id(2_011)
			.await
			.unwrap()
			.unwrap()
			.is_active,
		"the User stays inactive when the sessions could not be ended"
	);
	assert_eq!(events.len(), 1);
	assert_eq!(
		events[0].field("event"),
		Some("accounts.reactivate.refused")
	);
	assert_eq!(events[0].field("outcome"), Some("denied"));
	assert_eq!(events[0].field("reason"), Some("sessions_not_ended"));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_reactivating_an_active_user_changes_nothing_and_touches_no_session(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;
	let user = insert_user(2_012, "active", false).await;
	let session = redis.sessions.create(user.id).await.unwrap();

	// Act
	let (events, outcome) = capture_audit_events(reactivate(2_012, &redis.sessions)).await;

	// Assert
	assert_eq!(
		outcome.unwrap(),
		ReactivateOutcome::Unchanged { user_id: user.id }
	);
	assert_eq!(
		redis.sessions.resolve(&session.token).await.unwrap(),
		Some(user.id)
	);
	assert_eq!(
		events[0].field("event"),
		Some("accounts.reactivate.succeeded")
	);
	assert_eq!(events[0].field("reason"), Some("unchanged"));
}

#[rstest]
#[case::unknown_user(2_998, "unknown_user")]
#[case::non_positive(-3, "invalid_github_user_id")]
#[tokio::test]
#[serial(database)]
async fn sr_107_reactivation_refuses_an_invalid_or_unknown_id(
	#[future] database: TestDatabase,
	#[case] github_user_id: i64,
	#[case] reason: &'static str,
) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;

	// Act
	let (events, outcome) = capture_audit_events(reactivate(github_user_id, &redis.sessions)).await;

	// Assert
	assert!(outcome.is_err());
	assert_eq!(
		events[0].field("event"),
		Some("accounts.reactivate.refused")
	);
	assert_eq!(events[0].field("reason"), Some(reason));
}

/// The whole recovery over HTTP: the fallback deactivates the User and leaves the
/// session in Redis; reactivating must not bring that session back.
#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_107_a_session_left_behind_by_the_fallback_does_not_survive_reactivation() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let old = GithubAccount::new(4_201, "before");
	let new = GithubAccount::new(4_202, "after");
	app.expect_sign_in("code-before", &old).await;
	app.expect_sign_in("code-after", &new).await;
	let mut browser = app.browser();
	browser.sign_in("code-before").await;
	let stale_cookie = browser.cookie("cloud_session").unwrap().to_owned();
	let user = find_by_github_user_id(4_201).await.unwrap().unwrap();
	let failing = ScriptedRevoker::failing_after(1);
	repoint(4_201, 4_202, &failing).await.unwrap_err();
	assert!(
		!find_by_github_user_id(4_202)
			.await
			.unwrap()
			.unwrap()
			.is_active
	);
	let real = {
		use crate::apps::accounts::services::server::redis_handle::RedisHandle;
		use crate::apps::accounts::services::server::sessions::SessionService;
		use reinhardt::conf::settings::secret_types::SecretString;
		SessionService::new(RedisHandle::new(&SecretString::new(app.redis_url.clone())).unwrap())
	};

	// Act
	let outcome = reactivate(4_202, &real).await.unwrap();
	let mut replay = app.browser();
	replay.set_cookie("cloud_session", &stale_cookie);
	let replayed = replay
		.post_json(
			"/api/server_fn/current_viewer",
			json!({}),
			Some(&app.base_url),
		)
		.await;
	let mut fresh = app.browser();
	let signed_in = fresh.sign_in("code-after").await;

	// Assert
	assert!(matches!(outcome, ReactivateOutcome::Reactivated { .. }));
	assert_eq!(
		replayed.json(),
		json!(null),
		"the pre-existing session is refused after reactivation"
	);
	assert_eq!(signed_in.header("location"), Some("/"), "{signed_in:?}");
	assert_eq!(
		find_by_github_user_id(4_202).await.unwrap().unwrap().id,
		user.id
	);
}

/// The admin site is no way to reactivate a User: reinhardt-admin offers no hook
/// that could end the User's sessions first, so a reactivation there would bring
/// back every session left in Redis while the User was inactive.
#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_107_the_admin_site_cannot_reactivate_a_user_so_a_stale_session_stays_dead() {
	// Arrange: Staff and a target, both signed in; the target is then deactivated
	// (as the re-pointing fallback does), leaving their session in Redis.
	let app = TestApp::start(AppOptions::default()).await;
	insert_user(5_001, "ops", true).await;
	let ops = GithubAccount::new(5_001, "ops");
	let target = GithubAccount::new(5_002, "target");
	app.expect_sign_in("code-ops", &ops).await;
	app.expect_sign_in("code-target", &target).await;
	let mut staff = app.browser();
	staff.sign_in("code-ops").await;
	let mut stale = app.browser();
	stale.sign_in("code-target").await;
	let target_user = find_by_github_user_id(5_002).await.unwrap().unwrap();
	deactivate(&target_user).await;
	let dashboard = staff
		.post_with(
			"/admin/api/server_fn/get_dashboard",
			json!({}),
			&[("Origin", &app.base_url)],
		)
		.await;
	assert_eq!(dashboard.status, 200, "Staff reaches the admin site");
	let csrf = staff
		.cookie("csrftoken")
		.expect("the dashboard sets the CSRF cookie")
		.to_owned();
	let id = target_user.id.to_string();

	// Act
	let reactivate = staff
		.post_with(
			"/admin/api/server_fn/update_record",
			json!({"model_name": "User", "id": id, "request": {"csrf_token": csrf, "is_active": true}}),
			&[("Origin", &app.base_url)],
		)
		.await;
	let inline = staff
		.post_with(
			"/admin/api/server_fn/update_inline_edits",
			json!({"model_name": "User", "request": {"csrf_token": csrf, "updates": [
				{"object_id": id, "changes": {"is_active": true}}
			]}}),
			&[("Origin", &app.base_url)],
		)
		.await;
	let stale_viewer = stale
		.post_json(
			"/api/server_fn/current_viewer",
			json!({}),
			Some(&app.base_url),
		)
		.await;

	// Assert
	// Refused for permission, before the data is looked at (with change
	// permission the same requests get as far as the write).
	assert_eq!((reactivate.status, inline.status), (403, 403));
	for reply in [&reactivate, &inline] {
		assert_eq!(
			reply.json()["message"],
			json!("Permission denied"),
			"{reply:?}"
		);
	}
	assert!(
		!find_by_github_user_id(5_002)
			.await
			.unwrap()
			.unwrap()
			.is_active,
		"the admin site did not reactivate the User"
	);
	assert_eq!(
		stale_viewer.json(),
		json!(null),
		"the stale session stays refused"
	);
}
