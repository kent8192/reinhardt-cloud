//! Login Links at the service level: issuance, storage, lifetime, and atomic
//! single use (SR-16, SR-17, SR-18).

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use reinhardt::db::orm::Model;
use rstest::rstest;
use serial_test::serial;
use uuid::Uuid;

use crate::apps::accounts::models::{LoginLink, User};
use crate::apps::accounts::services::server::login_links::{
	ConsumeError, DEFAULT_LIFETIME, IssueError, LoginLinkSecret, MAX_LIFETIME, consume, issue,
	testing,
};
use crate::apps::accounts::tests::support::{TestDatabase, database, insert_user, user_count};
use crate::audit::capture::capture_audit_events;

async fn links_of(user_id: Uuid) -> Vec<LoginLink> {
	LoginLink::objects()
		.filter(LoginLink::field_user_id().eq(user_id))
		.all()
		.await
		.expect("links should be listed")
}

async fn deactivate(user: &User) {
	User::objects()
		.filter(User::field_id().eq(user.id))
		.update_fields([User::field_is_active().assign(false)])
		.await
		.expect("the User should be deactivated");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_18_a_login_link_is_stored_only_as_a_digest(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let user = insert_user(7_001, "operator", true).await;

	// Act
	let link = issue(7_001, DEFAULT_LIFETIME).await.unwrap();

	// Assert
	let rows = links_of(user.id).await;
	assert_eq!(rows.len(), 1);
	let secret = link.secret.expose();
	assert_eq!(secret.len(), 43, "256 bits as unpadded base64url");
	assert_eq!(rows[0].token_hash, testing::digest(&link.secret));
	assert_eq!(rows[0].token_hash.len(), 64);
	assert_ne!(rows[0].token_hash, secret);
	assert!(
		!format!("{:?}", rows[0]).contains(secret),
		"no column holds the secret"
	);
	assert_eq!(rows[0].user_id(), user.id);
	assert_eq!(rows[0].consumed_at, None);
	assert_eq!(rows[0].expires_at, link.expires_at);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_18_every_link_has_a_fresh_secret_and_the_secret_never_prints(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	insert_user(7_002, "operator", true).await;

	// Act
	let first = issue(7_002, DEFAULT_LIFETIME).await.unwrap();
	let second = issue(7_002, DEFAULT_LIFETIME).await.unwrap();

	// Assert
	assert_ne!(first.secret, second.secret);
	let debug = format!("{first:?}");
	assert!(!debug.contains(first.secret.expose()), "{debug}");
	assert!(debug.contains("<redacted>"), "{debug}");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_18_a_link_cannot_create_a_user(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;

	// Act
	let (events, result) = capture_audit_events(issue(404, DEFAULT_LIFETIME)).await;

	// Assert
	assert!(matches!(result, Err(IssueError::UnknownUser)));
	assert_eq!(user_count().await, 0);
	assert_eq!(LoginLink::objects().all().all().await.unwrap().len(), 0);
	assert_eq!(events.len(), 1);
	assert_eq!(
		events[0].field("event"),
		Some("accounts.login_link.issue_denied")
	);
	assert_eq!(events[0].field("reason"), Some("unknown_user"));
	assert_eq!(events[0].field("outcome"), Some("denied"));
	assert_eq!(events[0].field("actor_kind"), Some("host_operator"));
	assert_eq!(events[0].field("github_user_id"), Some("404"));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_18_a_link_is_refused_for_a_deactivated_user(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let user = insert_user(7_003, "gone", false).await;
	deactivate(&user).await;

	// Act
	let (events, result) = capture_audit_events(issue(7_003, DEFAULT_LIFETIME)).await;

	// Assert
	assert!(matches!(result, Err(IssueError::InactiveUser)));
	assert!(links_of(user.id).await.is_empty());
	assert_eq!(events[0].field("reason"), Some("user_inactive"));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_18_issuance_is_audited_without_the_secret(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let user = insert_user(7_004, "operator", true).await;

	// Act
	let (events, link) = capture_audit_events(issue(7_004, DEFAULT_LIFETIME)).await;

	// Assert
	let link = link.unwrap();
	assert_eq!(events.len(), 1);
	let issued = &events[0];
	assert_eq!(issued.field("event"), Some("accounts.login_link.issued"));
	assert_eq!(issued.field("actor_kind"), Some("host_operator"));
	assert_eq!(issued.field("outcome"), Some("succeeded"));
	assert_eq!(issued.field("github_user_id"), Some("7004"));
	assert_eq!(
		issued.field("subject_user_id"),
		Some(user.id.to_string().as_str())
	);
	let rendered = format!("{events:?}");
	assert!(!rendered.contains(link.secret.expose()), "{rendered}");
	assert!(
		!rendered.contains(&testing::digest(&link.secret)),
		"the digest is not logged either"
	);
}

#[rstest]
#[case::zero(Duration::ZERO)]
#[case::one_second_over_the_ceiling(MAX_LIFETIME + Duration::from_secs(1))]
#[case::a_day(Duration::from_secs(24 * 60 * 60))]
#[tokio::test]
#[serial(database)]
async fn sr_17_a_lifetime_beyond_the_ceiling_is_refused(
	#[future] database: TestDatabase,
	#[case] lifetime: Duration,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(7_005, "operator", true).await;

	// Act
	let result = issue(7_005, lifetime).await;

	// Assert
	assert!(matches!(result, Err(IssueError::InvalidLifetime)));
	assert!(links_of(user.id).await.is_empty());
}

#[rstest]
fn sr_17_the_ceiling_is_fifteen_minutes_and_the_default_is_within_it() {
	// Arrange / Act / Assert
	assert_eq!(MAX_LIFETIME, Duration::from_secs(15 * 60));
	assert_eq!(DEFAULT_LIFETIME, Duration::from_secs(10 * 60));
	assert!(DEFAULT_LIFETIME <= MAX_LIFETIME);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_17_the_expiry_is_fixed_at_issuance_from_the_requested_lifetime(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	insert_user(7_006, "operator", true).await;
	let before = Utc::now();

	// Act
	let link = issue(7_006, MAX_LIFETIME).await.unwrap();

	// Assert
	let after = Utc::now();
	let ceiling = ChronoDuration::from_std(MAX_LIFETIME).unwrap();
	assert!(link.expires_at >= before + ceiling - ChronoDuration::milliseconds(1));
	assert!(link.expires_at <= after + ceiling);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_16_a_link_is_consumed_exactly_once(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let user = insert_user(7_007, "operator", true).await;
	let link = issue(7_007, DEFAULT_LIFETIME).await.unwrap();

	// Act
	let (events, first) = capture_audit_events(consume(&link.secret)).await;
	let second = consume(&link.secret).await;

	// Assert
	assert_eq!(first.unwrap().id, user.id);
	assert!(matches!(second, Err(ConsumeError::Rejected)));
	let rows = links_of(user.id).await;
	assert!(rows[0].consumed_at.is_some());
	assert_eq!(events.len(), 1);
	assert_eq!(
		events[0].field("event"),
		Some("accounts.login_link.consumed")
	);
	assert_eq!(events[0].field("actor_kind"), Some("user"));
	assert_eq!(events[0].field("github_user_id"), Some("7007"));
	assert!(!format!("{events:?}").contains(link.secret.expose()));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_16_concurrent_consumption_has_exactly_one_winner(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	insert_user(7_008, "operator", true).await;
	let link = issue(7_008, DEFAULT_LIFETIME).await.unwrap();

	// Act
	let attempts: Vec<_> = (0..8)
		.map(|_| {
			let secret = link.secret.clone();
			tokio::spawn(async move { consume(&secret).await })
		})
		.collect();
	let mut winners = 0;
	for attempt in attempts {
		if attempt.await.expect("the task should not panic").is_ok() {
			winners += 1;
		}
	}

	// Assert
	assert_eq!(winners, 1, "exactly one concurrent caller may win");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_17_an_expired_link_is_rejected_by_the_server_clock(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let user = insert_user(7_009, "operator", true).await;
	let expired = testing::insert(user.id, Utc::now() - ChronoDuration::seconds(1), None).await;
	let live = testing::insert(user.id, Utc::now() + ChronoDuration::minutes(5), None).await;

	// Act
	let expired_result = consume(&expired).await;
	let live_result = consume(&live).await;

	// Assert
	assert!(matches!(expired_result, Err(ConsumeError::Rejected)));
	assert!(live_result.is_ok());
	let expired_row = LoginLink::objects()
		.filter(LoginLink::field_token_hash().eq(testing::digest(&expired)))
		.first()
		.await
		.unwrap()
		.unwrap();
	assert_eq!(
		expired_row.consumed_at, None,
		"an expired link is never marked used"
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_16_every_kind_of_bad_link_is_rejected_identically_but_audited_privately(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(7_010, "operator", true).await;
	let inactive = insert_user(7_011, "retired", false).await;
	deactivate(&inactive).await;
	let used = testing::insert(
		user.id,
		Utc::now() + ChronoDuration::minutes(5),
		Some(Utc::now()),
	)
	.await;
	let expired = testing::insert(user.id, Utc::now() - ChronoDuration::minutes(1), None).await;
	let of_inactive =
		testing::insert(inactive.id, Utc::now() + ChronoDuration::minutes(5), None).await;
	let unknown = LoginLinkSecret::from_input("not-a-link-anyone-issued");
	let empty = LoginLinkSecret::from_input("");
	let oversized = LoginLinkSecret::from_input(&"x".repeat(10_000));
	let cases = [
		(used, "used"),
		(expired, "expired"),
		(of_inactive, "user_inactive"),
		(unknown, "unknown"),
		(empty, "malformed"),
		(oversized, "malformed"),
	];

	for (secret, cause) in &cases {
		// Act
		let (events, result) = capture_audit_events(consume(secret)).await;

		// Assert
		let error = result.expect_err("a bad link signs nobody in");
		assert!(matches!(error, ConsumeError::Rejected), "{cause}");
		assert_eq!(
			error.to_string(),
			"the login link is not valid",
			"the caller-visible error is the same for `{cause}`"
		);
		assert_eq!(events.len(), 1, "{cause}");
		assert_eq!(
			events[0].field("event"),
			Some("accounts.login_link.rejected")
		);
		assert_eq!(events[0].field("outcome"), Some("denied"));
		assert_eq!(events[0].field("reason"), Some(*cause));
		assert!(
			!format!("{events:?}").contains(secret.expose()) || secret.expose().is_empty(),
			"the audit event holds no secret"
		);
	}
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_16_the_link_of_a_user_deactivated_after_issuance_is_burned_and_rejected(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(7_012, "operator", true).await;
	let link = issue(7_012, DEFAULT_LIFETIME).await.unwrap();
	deactivate(&user).await;

	// Act
	let first = consume(&link.secret).await;
	let rows = links_of(user.id).await;

	// Assert
	assert!(matches!(first, Err(ConsumeError::Rejected)));
	assert!(rows[0].consumed_at.is_some(), "the link is burned");
}
