//! Integration tests of the sign-up policy against the database (SR-19).

use rstest::rstest;
use serial_test::serial;

use crate::apps::accounts::models::{SocialAccount, User};
use crate::apps::accounts::services::server::sign_up_policy::{
	DenyReason, MembershipError, SignUpPolicy,
};
use crate::apps::accounts::services::server::users::{
	FirstSignInError, GithubProfile, ResolvedUser, resolve_first_sign_in,
};
use crate::apps::accounts::tests::support::{
	FixedMembership, TestDatabase, database, insert_user, profile, user_count,
};
use crate::audit::capture::capture_audit_events;
use reinhardt::db::orm::Model;

fn policy(name: &str, users: &str, organizations: &str) -> SignUpPolicy {
	SignUpPolicy::from_settings(name, users, organizations).unwrap()
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_19_invite_only_leaves_no_trace_for_an_unknown_identity(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;

	// Act
	let result = resolve_first_sign_in(
		&profile(900, "stranger"),
		&policy("invite_only", "", ""),
		&FixedMembership(Ok(vec![])),
	)
	.await;

	// Assert
	assert!(matches!(
		result,
		Err(FirstSignInError::Denied(DenyReason::NotInvited))
	));
	assert_eq!(user_count().await, 0);
	assert_eq!(SocialAccount::objects().all().all().await.unwrap().len(), 0);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_19_invite_only_admits_a_pre_provisioned_staff_user(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let staff = insert_user(1_000, "operator", true).await;

	// Act
	let result = resolve_first_sign_in(
		&profile(1_000, "operator"),
		&SignUpPolicy::InviteOnly,
		&FixedMembership(Err(MembershipError)),
	)
	.await
	.unwrap();

	// Assert
	let ResolvedUser::Existing(user) = result else {
		panic!("a pre-provisioned User must be found, not created");
	};
	assert_eq!((user.id, user.is_staff), (staff.id, true));
	assert_eq!(user_count().await, 1);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_19_tightening_the_policy_does_not_affect_existing_users(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let membership = FixedMembership(Ok(vec![]));
	let signed_up = resolve_first_sign_in(&profile(5, "early"), &SignUpPolicy::Open, &membership)
		.await
		.unwrap();

	// Act
	let again = resolve_first_sign_in(&profile(5, "early"), &SignUpPolicy::InviteOnly, &membership)
		.await
		.unwrap();

	// Assert
	assert_eq!(signed_up.user().id, again.user().id);
	assert!(again.user().is_active);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_19_open_policy_creates_a_user_with_profile_data(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let github_profile = GithubProfile {
		github_user_id: 321,
		login: "octocat".to_owned(),
		name: Some("The Octocat".to_owned()),
		avatar_url: Some("https://avatars.example.test/u/321".to_owned()),
		verified_email: Some("octocat@example.test".to_owned()),
	};

	// Act
	let resolved = resolve_first_sign_in(
		&github_profile,
		&SignUpPolicy::Open,
		&FixedMembership(Ok(vec![])),
	)
	.await
	.unwrap();

	// Assert
	let user = resolved.into_user();
	assert_eq!(user.github_user_id, 321);
	assert_eq!(user.github_login, "octocat");
	assert_eq!(user.display_name, "The Octocat");
	assert_eq!(user.email.as_deref(), Some("octocat@example.test"));
	assert!(user.is_active);
	assert!(!user.is_staff);
}

#[rstest]
#[case::listed_user(7, Ok(vec![]), None)]
#[case::member_of_listed_org(8, Ok(vec![100]), None)]
#[case::unrelated_user(9, Ok(vec![200]), Some(DenyReason::NotAllowlisted))]
#[case::membership_unavailable(9, Err(MembershipError), Some(DenyReason::MembershipUnverified))]
#[tokio::test]
#[serial(database)]
async fn sr_19_allowlist_admits_only_listed_identities(
	#[future] database: TestDatabase,
	#[case] github_user_id: i64,
	#[case] organizations: Result<Vec<i64>, MembershipError>,
	#[case] denial: Option<DenyReason>,
) {
	// Arrange
	let _db = database.await;
	let allowlist = policy("allowlist", "7", "100");

	// Act
	let result = resolve_first_sign_in(
		&profile(github_user_id, "candidate"),
		&allowlist,
		&FixedMembership(organizations),
	)
	.await;

	// Assert
	match denial {
		None => assert!(matches!(result, Ok(ResolvedUser::Created(_)))),
		Some(expected) => {
			assert!(matches!(result, Err(FirstSignInError::Denied(reason)) if reason == expected))
		}
	}
	assert_eq!(user_count().await, usize::from(denial.is_none()));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_19_denied_sign_up_is_audited_with_a_reason_code_only(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let mut stranger = profile(901, "stranger-login");
	stranger.verified_email = Some("stranger@example.test".to_owned());

	// Act
	let (events, _result) = capture_audit_events(async {
		resolve_first_sign_in(
			&stranger,
			&policy("invite_only", "", ""),
			&FixedMembership(Ok(vec![])),
		)
		.await
	})
	.await;

	// Assert
	assert_eq!(events.len(), 1);
	let event = &events[0];
	assert_eq!(event.field("event"), Some("accounts.sign_up.denied"));
	assert_eq!(event.field("outcome"), Some("denied"));
	assert_eq!(event.field("reason"), Some("not_invited"));
	assert_eq!(event.field("github_user_id"), Some("901"));
	let rendered = format!("{event:?}");
	assert!(!rendered.contains("stranger"));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_19_admitted_sign_up_is_audited(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;

	// Act
	let (events, result) = capture_audit_events(async {
		resolve_first_sign_in(
			&profile(902, "newcomer"),
			&SignUpPolicy::Open,
			&FixedMembership(Ok(vec![])),
		)
		.await
	})
	.await;

	// Assert
	let user: User = result.unwrap().into_user();
	assert_eq!(events.len(), 1);
	assert_eq!(events[0].field("event"), Some("accounts.sign_up.admitted"));
	assert_eq!(events[0].field("outcome"), Some("succeeded"));
	assert_eq!(events[0].field("reason"), Some("policy_open"));
	assert_eq!(
		events[0].field("subject_user_id"),
		Some(user.id.to_string().as_str())
	);
}
