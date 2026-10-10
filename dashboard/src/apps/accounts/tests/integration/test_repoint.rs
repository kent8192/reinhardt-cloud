//! Moving a User to another GitHub account (SR-107).

use reinhardt::conf::settings::secret_types::SecretString;
use reinhardt::db::orm::Model;
use rstest::rstest;
use serde_json::json;
use serial_test::serial;

use crate::apps::accounts::models::{LoginLink, SocialAccount, User};
use crate::apps::accounts::services::server::login_links::{DEFAULT_LIFETIME, consume, issue};
use crate::apps::accounts::services::server::provider_tokens::ProviderTokens;
use crate::apps::accounts::services::server::repoint::{
	Deactivation, RepointError, move_identity, repoint,
};
use crate::apps::accounts::services::server::users::find_by_github_user_id;
use crate::apps::accounts::tests::server_support::{AppOptions, GithubAccount, TestApp};
use crate::apps::accounts::tests::support::{
	ScriptedRevoker, TestDatabase, database, insert_user, redis_sessions, storage, user_count,
};
use crate::audit::capture::capture_audit_events;
use crate::persisted_time::persisted_now;

fn some_tokens() -> ProviderTokens {
	ProviderTokens {
		access_token: SecretString::new("ghu_old_identity_access"),
		refresh_token: Some(SecretString::new("ghr_old_identity_refresh")),
		access_token_expires_at: None,
		refresh_token_expires_at: None,
	}
}

async fn make_signed_in(user: &User) {
	User::objects()
		.filter(User::field_id().eq(user.id))
		.update_fields([
			User::field_last_login().assign(Some(persisted_now())),
			User::field_display_name().assign("Old Name".to_owned()),
			User::field_avatar_url().assign(Some("https://avatars.example.test/old".to_owned())),
			User::field_email().assign(Some("old@example.test".to_owned())),
		])
		.await
		.unwrap();
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_the_user_moves_and_keeps_their_identity_inside_the_control_plane(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;
	let user = insert_user(1_000, "old-login", true).await;
	make_signed_in(&user).await;

	// Act
	let done = repoint(1_000, 2_000, &redis.sessions).await.unwrap();

	// Assert
	assert_eq!(done.user_id, user.id);
	assert!(find_by_github_user_id(1_000).await.unwrap().is_none());
	let moved = find_by_github_user_id(2_000).await.unwrap().unwrap();
	assert_eq!(
		moved.id, user.id,
		"the internal ID, and so the Memberships, stay"
	);
	assert!(
		moved.is_staff && moved.is_active,
		"Staff and activation stay"
	);
	assert!(moved.last_login.is_some());
	assert_eq!(moved.github_login, "github-2000");
	assert_eq!(moved.display_name, "github-2000");
	assert_eq!(
		(moved.avatar_url, moved.email),
		(None, None),
		"nothing of the old account's profile lingers"
	);
	assert_eq!(user_count().await, 1);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_every_session_of_the_user_ends(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;
	let user = insert_user(1_001, "sessions", false).await;
	let other = insert_user(1_002, "bystander", false).await;
	let mine = [
		redis.sessions.create(user.id).await.unwrap(),
		redis.sessions.create(user.id).await.unwrap(),
		redis.sessions.create(user.id).await.unwrap(),
	];
	let theirs = redis.sessions.create(other.id).await.unwrap();

	// Act
	let done = repoint(1_001, 3_001, &redis.sessions).await.unwrap();

	// Assert
	assert_eq!(done.sessions_ended_before, 3);
	for session in &mine {
		assert_eq!(redis.sessions.resolve(&session.token).await.unwrap(), None);
	}
	assert_eq!(
		redis.sessions.resolve(&theirs.token).await.unwrap(),
		Some(other.id),
		"another User's session is untouched"
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_the_stored_provider_tokens_of_the_old_identity_are_cleared(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;
	let user = insert_user(1_003, "tokens", false).await;
	let tokens = storage();
	tokens.store_tokens(user.id, &some_tokens()).await.unwrap();
	assert!(tokens.load_tokens(user.id).await.unwrap().is_some());

	// Act
	repoint(1_003, 3_003, &redis.sessions).await.unwrap();

	// Assert
	assert!(tokens.load_tokens(user.id).await.unwrap().is_none());
	assert!(
		SocialAccount::objects()
			.all()
			.all()
			.await
			.unwrap()
			.is_empty()
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_unused_login_links_do_not_survive_the_move(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;
	insert_user(1_004, "links", true).await;
	let link = issue(1_004, DEFAULT_LIFETIME).await.unwrap();

	// Act
	repoint(1_004, 3_004, &redis.sessions).await.unwrap();
	let result = consume(&link.secret).await;

	// Assert
	assert!(
		result.is_err(),
		"a link issued for the old arrangement is dead"
	);
	assert!(LoginLink::objects().all().all().await.unwrap().is_empty());
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_an_id_owned_by_another_user_is_refused_and_nothing_changes(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;
	let mover = insert_user(1_005, "mover", true).await;
	let owner = insert_user(1_006, "owner", false).await;
	let session = redis.sessions.create(mover.id).await.unwrap();
	let tokens = storage();
	tokens.store_tokens(mover.id, &some_tokens()).await.unwrap();

	// Act
	let (events, result) = capture_audit_events(repoint(1_005, 1_006, &redis.sessions)).await;

	// Assert
	assert!(matches!(result, Err(RepointError::TargetInUse)));
	assert_eq!(
		find_by_github_user_id(1_005).await.unwrap().unwrap().id,
		mover.id
	);
	assert_eq!(
		find_by_github_user_id(1_006).await.unwrap().unwrap().id,
		owner.id
	);
	assert_eq!(
		redis.sessions.resolve(&session.token).await.unwrap(),
		Some(mover.id),
		"a refused move does not end sessions"
	);
	assert!(tokens.load_tokens(mover.id).await.unwrap().is_some());
	assert_eq!(events.len(), 1);
	assert_eq!(events[0].field("event"), Some("accounts.repoint.denied"));
	assert_eq!(events[0].field("reason"), Some("target_in_use"));
	assert_eq!(events[0].field("outcome"), Some("denied"));
	assert_eq!(events[0].field("github_user_id"), Some("1006"));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_losing_the_unique_race_for_the_target_is_the_same_refusal(
	#[future] database: TestDatabase,
) {
	// Arrange: the target is taken after the up-front check would have passed.
	let _db = database.await;
	let mover = insert_user(1_007, "mover", true).await;
	insert_user(1_008, "owner", false).await;
	let tokens = storage();
	tokens.store_tokens(mover.id, &some_tokens()).await.unwrap();

	// Act
	let result = move_identity(mover.id, 1_007, 1_008).await;

	// Assert
	assert!(matches!(result, Err(RepointError::TargetInUse)));
	assert_eq!(
		find_by_github_user_id(1_007).await.unwrap().unwrap().id,
		mover.id,
		"the transaction rolled back"
	);
	assert!(tokens.load_tokens(mover.id).await.unwrap().is_some());
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_a_user_changed_underneath_the_move_rolls_everything_back(
	#[future] database: TestDatabase,
) {
	// Arrange: the User no longer has the ID the operator named.
	let _db = database.await;
	let user = insert_user(1_009, "moved-already", true).await;
	let tokens = storage();
	tokens.store_tokens(user.id, &some_tokens()).await.unwrap();

	// Act
	let result = move_identity(user.id, 1_999, 3_009).await;

	// Assert
	assert!(matches!(result, Err(RepointError::Storage(_))));
	assert_eq!(
		find_by_github_user_id(1_009).await.unwrap().unwrap().id,
		user.id
	);
	assert!(
		tokens.load_tokens(user.id).await.unwrap().is_some(),
		"tokens of an identity that was not the named one are not touched"
	);
}

#[rstest]
#[case::unknown_user(1_100, 3_100, "unknown_user")]
#[case::same_id(1_101, 1_101, "same_github_user_id")]
#[case::non_positive_source(0, 3_102, "invalid_github_user_id")]
#[case::non_positive_target(1_103, -5, "invalid_github_user_id")]
#[tokio::test]
#[serial(database)]
async fn sr_107_invalid_requests_are_denied_with_a_reason(
	#[future] database: TestDatabase,
	#[case] from: i64,
	#[case] to: i64,
	#[case] reason: &'static str,
) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;
	insert_user(1_101, "present", false).await;
	insert_user(1_103, "present-too", false).await;

	// Act
	let (events, result) = capture_audit_events(repoint(from, to, &redis.sessions)).await;

	// Assert
	assert!(result.is_err());
	assert_eq!(events.len(), 1);
	assert_eq!(events[0].field("event"), Some("accounts.repoint.denied"));
	assert_eq!(events[0].field("reason"), Some(reason));
	assert_eq!(user_count().await, 2);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_the_audit_events_name_both_identifiers_and_hold_no_secret(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;
	let user = insert_user(1_200, "audited", false).await;
	make_signed_in(&user).await;
	storage()
		.store_tokens(user.id, &some_tokens())
		.await
		.unwrap();

	// Act
	let (events, done) = capture_audit_events(repoint(1_200, 3_200, &redis.sessions)).await;

	// Assert
	done.unwrap();
	assert_eq!(events.len(), 2);
	let ids: Vec<_> = events
		.iter()
		.map(|event| (event.field("event"), event.field("github_user_id")))
		.collect();
	assert_eq!(
		ids,
		[
			(Some("accounts.repoint.released"), Some("1200")),
			(Some("accounts.repoint.claimed"), Some("3200")),
		]
	);
	for event in &events {
		assert_eq!(event.field("actor_kind"), Some("host_operator"));
		assert_eq!(event.field("outcome"), Some("succeeded"));
		assert_eq!(
			event.field("subject_user_id"),
			Some(user.id.to_string().as_str())
		);
	}
	let rendered = format!("{events:?}");
	for secret in [
		"ghu_old_identity_access",
		"ghr_old_identity_refresh",
		"old@example.test",
	] {
		assert!(!rendered.contains(secret), "{rendered}");
	}
}

/// After the move the old GitHub account can no longer reach the User, and the
/// new one reaches the same User with a fresh profile.
#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_107_the_old_account_loses_the_user_and_the_new_account_gets_it() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let redis_sessions = {
		use crate::apps::accounts::services::server::redis_handle::RedisHandle;
		use crate::apps::accounts::services::server::sessions::SessionService;
		SessionService::new(RedisHandle::new(&SecretString::new(app.redis_url.clone())).unwrap())
	};
	let old = GithubAccount::new(4_001, "old-account");
	let new = GithubAccount::new(4_002, "new-account");
	app.expect_sign_in("code-old", &old).await;
	app.expect_sign_in("code-new", &new).await;
	app.expect_sign_in("code-old-again", &old).await;
	let mut browser = app.browser();
	browser.sign_in("code-old").await;
	let original = find_by_github_user_id(4_001).await.unwrap().unwrap();
	let viewer = |reply: crate::apps::accounts::tests::server_support::Reply| reply.json();

	// Act
	repoint(4_001, 4_002, &redis_sessions).await.unwrap();
	let stale = browser
		.post_json(
			"/api/server_fn/current_viewer",
			json!({}),
			Some(&app.base_url),
		)
		.await;
	let mut newcomer = app.browser();
	let new_sign_in = newcomer.sign_in("code-new").await;
	let mut returning_old = app.browser();
	let old_sign_in = returning_old.sign_in("code-old-again").await;

	// Assert
	assert_eq!(
		viewer(stale),
		json!(null),
		"the session ended with the move"
	);
	assert_eq!(new_sign_in.header("location"), Some("/"), "{new_sign_in:?}");
	let reached = find_by_github_user_id(4_002).await.unwrap().unwrap();
	assert_eq!(reached.id, original.id, "the new account is the same User");
	assert_eq!(reached.github_login, "new-account", "the profile filled in");
	assert_eq!(old_sign_in.header("location"), Some("/"), "{old_sign_in:?}");
	let impostor = find_by_github_user_id(4_001).await.unwrap().unwrap();
	assert_ne!(
		impostor.id, original.id,
		"under an open policy the old account is a brand-new, unrelated User"
	);
	assert!(!impostor.is_staff);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_a_redis_failure_after_the_commit_still_records_the_move_and_fails_closed(
	#[future] database: TestDatabase,
) {
	// Arrange: the first session pass works, the one after the commit fails.
	let _db = database.await;
	let user = insert_user(1_300, "committed", true).await;
	let revoker = ScriptedRevoker::failing_after(1);

	// Act
	let (events, result) = capture_audit_events(repoint(1_300, 3_300, &revoker)).await;

	// Assert
	let error = result.expect_err("the cleanup failed, so the command fails");
	assert!(
		matches!(
			error,
			RepointError::SessionsAfterChange {
				deactivation: Deactivation::Done,
				..
			}
		),
		"{error:?}"
	);
	let message = error.to_string();
	assert!(message.contains("was deactivated"), "{message}");
	assert!(message.contains("reactivate"), "{message}");
	assert!(message.contains("`is_active`"), "{message}");

	let moved = find_by_github_user_id(3_300).await.unwrap().unwrap();
	assert_eq!(moved.id, user.id, "the committed move stands");
	assert!(!moved.is_active, "fail closed: the User is deactivated");
	assert!(moved.is_staff, "Staff is not touched by the fallback");

	let recorded: Vec<_> = events
		.iter()
		.map(|event| {
			(
				event.field("event"),
				event.field("reason"),
				event.field("github_user_id"),
			)
		})
		.collect();
	assert_eq!(
		recorded,
		[
			(Some("accounts.repoint.released"), None, Some("1300")),
			(Some("accounts.repoint.claimed"), None, Some("3300")),
			(
				Some("accounts.repoint.deactivated"),
				Some("sessions_not_ended_after_change"),
				Some("3300")
			),
			(
				Some("accounts.repoint.failed"),
				Some("sessions_not_ended_after_change"),
				Some("3300")
			),
		]
	);
	let outcomes: Vec<_> = events.iter().map(|event| event.field("outcome")).collect();
	assert_eq!(
		outcomes,
		[
			Some("succeeded"),
			Some("succeeded"),
			Some("succeeded"),
			Some("failed")
		]
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_107_a_redis_failure_before_anything_changed_changes_nothing(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(1_301, "untouched", true).await;
	let revoker = ScriptedRevoker::failing_after(0);

	// Act
	let (events, result) = capture_audit_events(repoint(1_301, 3_301, &revoker)).await;

	// Assert
	assert!(matches!(result, Err(RepointError::SessionsBeforeChange(_))));
	let same = find_by_github_user_id(1_301).await.unwrap().unwrap();
	assert_eq!((same.id, same.is_active), (user.id, true));
	assert!(find_by_github_user_id(3_301).await.unwrap().is_none());
	assert_eq!(
		events.len(),
		1,
		"no success event for a move that did not happen"
	);
	assert_eq!(events[0].field("event"), Some("accounts.repoint.failed"));
	assert_eq!(
		events[0].field("reason"),
		Some("sessions_not_ended_before_change")
	);
}

#[rstest]
fn sr_107_the_message_tells_the_operator_what_to_do_when_deactivation_failed_too() {
	// Arrange
	let error = RepointError::SessionsAfterChange {
		cause: "connection refused".to_owned(),
		deactivation: Deactivation::Failed("the database is gone".to_owned()),
	};

	// Act
	let message = error.to_string();

	// Assert
	assert!(
		message.contains("could not be deactivated either"),
		"{message}"
	);
	assert!(
		message.contains("deactivate the User in the admin site now"),
		"{message}"
	);
	assert!(message.contains("the database is gone"), "{message}");
}

/// The leftover session of a User whose cleanup failed is refused on its next
/// request, without Redis having to be reachable for the check to be correct.
#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_107_a_leftover_session_is_refused_on_its_next_request_after_a_failed_cleanup() {
	// Arrange
	let app = TestApp::start(AppOptions::default()).await;
	let old = GithubAccount::new(4_101, "leftover");
	app.expect_sign_in("code-leftover", &old).await;
	let mut browser = app.browser();
	browser.sign_in("code-leftover").await;
	let user = find_by_github_user_id(4_101).await.unwrap().unwrap();
	let before = browser
		.post_json(
			"/api/server_fn/current_viewer",
			json!({}),
			Some(&app.base_url),
		)
		.await;
	assert_eq!(before.json()["github_login"], json!("leftover"));
	// The session survives the failed cleanup: both passes are no-ops or errors.
	let revoker = ScriptedRevoker::failing_after(1);

	// Act
	let result = repoint(4_101, 4_102, &revoker).await;
	let after = browser
		.post_json(
			"/api/server_fn/current_viewer",
			json!({}),
			Some(&app.base_url),
		)
		.await;

	// Assert
	assert!(matches!(
		result,
		Err(RepointError::SessionsAfterChange { .. })
	));
	let moved = find_by_github_user_id(4_102).await.unwrap().unwrap();
	assert_eq!((moved.id, moved.is_active), (user.id, false));
	assert_eq!(
		after.json(),
		json!(null),
		"the leftover session no longer authenticates anyone"
	);
}
