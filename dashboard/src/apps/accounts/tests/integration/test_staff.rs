//! Staff grants, revocations, and pre-provisioning (SR-20, SR-105).

use reinhardt::db::orm::Model;
use rstest::rstest;
use serde_json::json;
use serial_test::serial;

use crate::apps::accounts::models::User;
use crate::apps::accounts::services::server::staff::{
	GrantOutcome, RevokeOutcome, StaffError, grant, placeholder_login, revoke,
};
use crate::apps::accounts::services::server::users::find_by_github_user_id;
use crate::apps::accounts::tests::server_support::{AppOptions, GithubAccount, TestApp};
use crate::apps::accounts::tests::support::{
	TestDatabase, database, insert_user, redis_sessions, user_count,
};
use crate::audit::capture::capture_audit_events;
use crate::persisted_time::persisted_now;

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_105_a_grant_pre_provisions_a_staff_user_from_the_id_alone(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;

	// Act
	let (events, outcome) = capture_audit_events(grant(31_337)).await;

	// Assert
	let GrantOutcome::PreProvisioned { user_id } = outcome.unwrap() else {
		panic!("an unknown GitHub ID must be pre-provisioned");
	};
	let user = find_by_github_user_id(31_337).await.unwrap().unwrap();
	assert_eq!(user.id, user_id);
	assert!(user.is_staff && user.is_active);
	assert_eq!(user.last_login, None, "nobody has signed in as this User");
	assert_eq!(user.github_login, placeholder_login(31_337));
	assert_eq!(user.display_name, "github-31337");
	assert_eq!((user.avatar_url, user.email), (None, None));
	assert_eq!(user_count().await, 1);
	assert_eq!(events.len(), 1);
	assert_eq!(
		events[0].field("event"),
		Some("accounts.grant_staff.succeeded")
	);
	assert_eq!(events[0].field("actor_kind"), Some("host_operator"));
	assert_eq!(events[0].field("github_user_id"), Some("31337"));
	assert_eq!(events[0].field("reason"), Some("pre_provisioned"));
	assert_eq!(events[0].field("outcome"), Some("succeeded"));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_20_a_grant_promotes_an_existing_user_and_is_idempotent(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(41, "regular", false).await;

	// Act
	let (events, first) = capture_audit_events(grant(41)).await;
	let second = grant(41).await;

	// Assert
	assert_eq!(
		first.unwrap(),
		GrantOutcome::Granted {
			user_id: user.id,
			deactivated: false
		}
	);
	assert_eq!(
		second.unwrap(),
		GrantOutcome::AlreadyStaff { user_id: user.id }
	);
	assert!(find_by_github_user_id(41).await.unwrap().unwrap().is_staff);
	assert_eq!(user_count().await, 1);
	assert_eq!(events[0].field("reason"), Some("existing_user"));
	assert_eq!(events[0].field("github_user_id"), Some("41"));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_20_a_grant_to_a_deactivated_user_says_they_still_cannot_sign_in(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(42, "retired", false).await;
	User::objects()
		.filter(User::field_id().eq(user.id))
		.update_fields([User::field_is_active().assign(false)])
		.await
		.unwrap();

	// Act
	let outcome = grant(42).await.unwrap();

	// Assert
	assert_eq!(
		outcome,
		GrantOutcome::Granted {
			user_id: user.id,
			deactivated: true
		}
	);
	assert!(!find_by_github_user_id(42).await.unwrap().unwrap().is_active);
}

#[rstest]
#[case::zero(0)]
#[case::negative(-7)]
#[tokio::test]
#[serial(database)]
async fn sr_20_a_grant_needs_a_positive_numeric_id(
	#[future] database: TestDatabase,
	#[case] github_user_id: i64,
) {
	// Arrange
	let _db = database.await;

	// Act
	let (events, outcome) = capture_audit_events(grant(github_user_id)).await;

	// Assert
	assert!(matches!(outcome, Err(StaffError::InvalidGithubUserId)));
	assert_eq!(user_count().await, 0);
	assert_eq!(
		events[0].field("event"),
		Some("accounts.grant_staff.denied")
	);
	assert_eq!(events[0].field("outcome"), Some("denied"));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_20_concurrent_grants_for_one_new_id_create_one_user(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;

	// Act
	let attempts: Vec<_> = (0..6).map(|_| tokio::spawn(grant(555))).collect();
	for attempt in attempts {
		attempt.await.unwrap().unwrap();
	}

	// Assert
	assert_eq!(user_count().await, 1);
	assert!(find_by_github_user_id(555).await.unwrap().unwrap().is_staff);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_20_a_revocation_removes_staff_and_ends_every_session(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;
	let user = insert_user(51, "was-staff", true).await;
	User::objects()
		.filter(User::field_id().eq(user.id))
		.update_fields([User::field_last_login().assign(Some(persisted_now()))])
		.await
		.unwrap();
	let first = redis.sessions.create(user.id).await.unwrap();
	let second = redis.sessions.create(user.id).await.unwrap();

	// Act
	let (events, outcome) = capture_audit_events(revoke(51, &redis.sessions)).await;

	// Assert
	assert_eq!(
		outcome.unwrap(),
		RevokeOutcome::Revoked {
			user_id: user.id,
			sessions_ended: 2
		}
	);
	let after = find_by_github_user_id(51).await.unwrap().unwrap();
	assert!(
		!after.is_staff && after.is_active,
		"the User stays, as a regular User"
	);
	assert_eq!(redis.sessions.resolve(&first.token).await.unwrap(), None);
	assert_eq!(redis.sessions.resolve(&second.token).await.unwrap(), None);
	assert_eq!(events.len(), 1);
	assert_eq!(
		events[0].field("event"),
		Some("accounts.revoke_staff.succeeded")
	);
	assert_eq!(events[0].field("reason"), Some("revoked"));
	assert_eq!(events[0].field("github_user_id"), Some("51"));
	assert_eq!(events[0].field("actor_kind"), Some("host_operator"));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_105_revoking_a_pre_provisioned_user_who_never_signed_in_removes_the_exemption(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;
	grant(61).await.unwrap();

	// Act
	let outcome = revoke(61, &redis.sessions).await.unwrap();

	// Assert
	assert!(matches!(outcome, RevokeOutcome::PreProvisionRemoved { .. }));
	assert!(find_by_github_user_id(61).await.unwrap().is_none());
	assert_eq!(user_count().await, 0);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_20_a_revocation_of_an_unknown_id_is_denied_and_changes_nothing(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;
	insert_user(71, "bystander", true).await;

	// Act
	let (events, outcome) = capture_audit_events(revoke(72, &redis.sessions)).await;

	// Assert
	assert!(matches!(outcome, Err(StaffError::UnknownUser)));
	assert!(find_by_github_user_id(71).await.unwrap().unwrap().is_staff);
	assert_eq!(
		events[0].field("event"),
		Some("accounts.revoke_staff.denied")
	);
	assert_eq!(events[0].field("reason"), Some("unknown_user"));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_20_revoking_a_regular_user_still_ends_their_sessions(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let redis = redis_sessions().await;
	let user = insert_user(81, "regular", false).await;
	let session = redis.sessions.create(user.id).await.unwrap();

	// Act
	let outcome = revoke(81, &redis.sessions).await.unwrap();

	// Assert
	assert_eq!(
		outcome,
		RevokeOutcome::NotStaff {
			user_id: user.id,
			sessions_ended: 1
		}
	);
	assert_eq!(redis.sessions.resolve(&session.token).await.unwrap(), None);
}

/// A pre-provisioned account signs in under `invite_only`, whatever the policy.
#[rstest]
#[tokio::test]
#[serial(database, env_settings_load)]
async fn sr_105_a_pre_provisioned_account_signs_in_under_invite_only_and_nobody_else_does() {
	// Arrange
	let app = TestApp::start(AppOptions {
		sign_up_policy: "invite_only",
		..AppOptions::default()
	})
	.await;
	let pre_provisioned = match grant(9_001).await.unwrap() {
		GrantOutcome::PreProvisioned { user_id } => user_id,
		other => panic!("expected a pre-provisioned User, got {other:?}"),
	};
	let mut operator = GithubAccount::new(9_001, "first-operator");
	operator.name = Some("First Operator");
	let stranger = GithubAccount::new(9_002, "stranger");
	app.expect_sign_in("code-operator", &operator).await;
	app.expect_sign_in("code-stranger", &stranger).await;

	// Act
	let mut operator_browser = app.browser();
	let (events, callback) = capture_audit_events(operator_browser.sign_in("code-operator")).await;
	let mut stranger_browser = app.browser();
	let refused = stranger_browser.sign_in("code-stranger").await;

	// Assert
	assert_eq!(callback.header("location"), Some("/"), "{callback:?}");
	let user = find_by_github_user_id(9_001).await.unwrap().unwrap();
	assert_eq!(user.id, pre_provisioned, "the existing row was used");
	assert!(user.is_staff);
	assert_eq!(user.github_login, "first-operator", "the profile filled in");
	assert_eq!(user.display_name, "First Operator");
	assert!(user.last_login.is_some());
	assert!(
		events
			.iter()
			.all(|event| event.field("event") != Some("accounts.sign_up.admitted")),
		"no sign-up happened: the policy was not consulted"
	);
	let viewer = operator_browser
		.post_json(
			"/api/server_fn/current_viewer",
			json!({}),
			Some(&app.base_url),
		)
		.await;
	assert_eq!(viewer.json()["is_staff"], json!(true));

	assert_eq!(refused.header("location"), Some("/sign-in/"), "{refused:?}");
	assert!(
		find_by_github_user_id(9_002).await.unwrap().is_none(),
		"the exemption is for the named ID only"
	);
}
