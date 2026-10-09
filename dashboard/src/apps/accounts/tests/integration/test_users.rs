//! Integration tests of User identity (SR-02, SR-03).

use reinhardt::core::exception::DatabaseErrorKind;
use reinhardt::db::orm::Model;
use rstest::rstest;
use serial_test::serial;

use crate::apps::accounts::models::{SocialAccount, User};
use crate::apps::accounts::services::server::provider_tokens::ProviderTokens;
use crate::apps::accounts::services::server::sign_up_policy::SignUpPolicy;
use crate::apps::accounts::services::server::users::{
	ResolvedUser, UserError, find_by_github_user_id, resolve_first_sign_in, sync_profile,
};
use crate::apps::accounts::tests::support::{
	FixedMembership, TestDatabase, database, database_violation, insert_user, profile, storage,
	user_count,
};
use chrono::{Duration, Utc};
use reinhardt::conf::settings::secret_types::SecretString;

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_02_renamed_login_updates_the_same_user(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let original = insert_user(4_242, "alice", true).await;
	let renamed = profile(4_242, "alice-renamed");

	// Act
	let synced = sync_profile(original.clone(), &renamed).await.unwrap();

	// Assert
	assert_eq!(synced.id, original.id);
	assert_eq!(synced.github_login, "alice-renamed");
	assert_eq!(synced.display_name, "alice-renamed");
	assert!(synced.is_staff, "a profile sync must not change Staff");
	assert!(synced.is_active);
	assert_eq!(user_count().await, 1);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_02_login_reclaimed_by_another_account_never_reaches_the_old_user(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let original = insert_user(1, "alice", true).await;
	let newcomer = profile(2, "alice");

	// Act
	let resolved =
		resolve_first_sign_in(&newcomer, &SignUpPolicy::Open, &FixedMembership(Ok(vec![])))
			.await
			.unwrap();

	// Assert
	let ResolvedUser::Created(created) = resolved else {
		panic!("a new GitHub ID must create a new User");
	};
	assert_ne!(created.id, original.id);
	assert!(
		!created.is_staff,
		"the old User's Staff must not be inherited"
	);
	assert_eq!(user_count().await, 2);
	let old = find_by_github_user_id(1).await.unwrap().unwrap();
	assert_eq!((old.id, old.is_staff), (original.id, true));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_02_email_is_never_a_lookup_key(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let mut first = profile(10, "first");
	first.verified_email = Some("shared@example.test".to_owned());
	let mut second = profile(11, "second");
	second.verified_email = Some("shared@example.test".to_owned());
	let membership = FixedMembership(Ok(vec![]));

	// Act
	let a = resolve_first_sign_in(&first, &SignUpPolicy::Open, &membership)
		.await
		.unwrap();
	let b = resolve_first_sign_in(&second, &SignUpPolicy::Open, &membership)
		.await
		.unwrap();

	// Assert
	assert_ne!(a.user().id, b.user().id);
	assert_eq!(user_count().await, 2);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_02_profile_of_another_identity_is_not_applied(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let user = insert_user(1, "alice", false).await;

	// Act
	let result = sync_profile(user, &profile(2, "mallory")).await;

	// Assert
	assert!(matches!(result, Err(UserError::InvalidProfile)));
	let stored = find_by_github_user_id(1).await.unwrap().unwrap();
	assert_eq!(stored.github_login, "alice");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_03_storage_rejects_a_second_user_for_one_github_identity(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	insert_user(77, "first", false).await;
	let duplicate = User::build()
		.github_user_id(77)
		.github_login("second".to_owned())
		.display_name("second".to_owned())
		.avatar_url(None)
		.email(None)
		.is_active(true)
		.is_staff(false)
		.finish();

	// Act
	let result = User::objects().create(&duplicate).await;

	// Assert
	let error = result.unwrap_err();
	assert_eq!(
		database_violation(&error),
		Some((
			DatabaseErrorKind::UniqueViolation,
			Some("accounts_users_github_user_id_uniq".to_owned())
		))
	);
	assert_eq!(user_count().await, 1);
}

#[rstest]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial(database)]
async fn sr_03_concurrent_first_sign_ins_create_exactly_one_user(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let github_profile = profile(555, "racer");

	// Act
	let attempts = (0..8).map(|_| {
		let github_profile = github_profile.clone();
		tokio::spawn(async move {
			resolve_first_sign_in(
				&github_profile,
				&SignUpPolicy::Open,
				&FixedMembership(Ok(vec![])),
			)
			.await
		})
	});
	let mut ids = Vec::new();
	for attempt in attempts {
		ids.push(attempt.await.unwrap().unwrap().into_user().id);
	}

	// Assert
	assert_eq!(user_count().await, 1);
	assert!(ids.iter().all(|id| *id == ids[0]));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_03_repeating_the_same_sign_in_is_idempotent(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let github_profile = profile(9, "repeat");
	let membership = FixedMembership(Ok(vec![]));

	// Act
	let first = resolve_first_sign_in(&github_profile, &SignUpPolicy::Open, &membership)
		.await
		.unwrap();
	let second = resolve_first_sign_in(&github_profile, &SignUpPolicy::Open, &membership)
		.await
		.unwrap();

	// Assert
	assert!(matches!(first, ResolvedUser::Created(_)));
	assert!(matches!(second, ResolvedUser::Existing(_)));
	assert_eq!(first.user().id, second.user().id);
	assert_eq!(user_count().await, 1);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_03_a_user_holds_exactly_one_set_of_provider_tokens(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let user = insert_user(31, "tokens", false).await;
	let storage = storage();
	let tokens = |label: &str| ProviderTokens {
		access_token: SecretString::new(format!("access-{label}")),
		refresh_token: Some(SecretString::new(format!("refresh-{label}"))),
		access_token_expires_at: Utc::now() + Duration::hours(8),
		refresh_token_expires_at: Some(Utc::now() + Duration::days(180)),
	};

	// Act
	storage.store_tokens(user.id, &tokens("one")).await.unwrap();
	storage.store_tokens(user.id, &tokens("two")).await.unwrap();

	// Assert
	let rows = SocialAccount::objects().all().all().await.unwrap();
	assert_eq!(rows.len(), 1);
	let loaded = storage.load_tokens(user.id).await.unwrap().unwrap();
	assert_eq!(loaded.access_token.expose_secret(), "access-two");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_03_the_database_rejects_a_second_social_account_row_for_one_user(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(32, "twice", false).await;
	let row = || {
		SocialAccount::build()
			.user(user.id)
			.encrypted_access_token("v1.primary.AAAAAAAAAAAAAAAAAAAA".to_owned())
			.encrypted_refresh_token(None)
			.access_token_expires_at(Utc::now() + Duration::hours(8))
			.refresh_token_expires_at(None)
			.finish()
	};
	SocialAccount::objects().create(&row()).await.unwrap();

	// Act
	let second = SocialAccount::objects().create(&row()).await;

	// Assert
	let error = second.unwrap_err();
	assert_eq!(
		database_violation(&error).map(|(kind, _constraint)| kind),
		Some(DatabaseErrorKind::UniqueViolation)
	);
	assert_eq!(SocialAccount::objects().all().all().await.unwrap().len(), 1);
}
