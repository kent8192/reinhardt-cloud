//! Integration tests of provider token storage (SR-06, SR-03).

use chrono::{DateTime, Duration, SubsecRound, Utc};
use reinhardt::auth::social::core::SocialAuthError;
use reinhardt::auth::social::storage::{SocialAccount as UpstreamAccount, SocialAccountStorage};
use reinhardt::conf::settings::secret_types::SecretString;
use reinhardt::db::orm::Model;
use rstest::rstest;
use serial_test::serial;
use uuid::Uuid;

use crate::apps::accounts::models::SocialAccount as StoredAccount;
use crate::apps::accounts::services::server::provider_tokens::{
	OrmSocialAccountStorage, ProviderTokenError, ProviderTokens,
};
use crate::apps::accounts::services::server::token_crypto::TokenCryptoError;
use crate::apps::accounts::tests::support::{
	OTHER_TEST_KEY, TestDatabase, database, insert_user, keyring, storage,
};

fn tokens(access: &str, refresh: Option<&str>) -> ProviderTokens {
	ProviderTokens {
		access_token: SecretString::new(access),
		refresh_token: refresh.map(SecretString::new),
		access_token_expires_at: Utc::now().trunc_subsecs(3) + Duration::hours(8),
		refresh_token_expires_at: refresh
			.map(|_| Utc::now().trunc_subsecs(3) + Duration::days(180)),
	}
}

fn millis(time: DateTime<Utc>) -> i64 {
	time.timestamp_millis()
}

fn upstream_account(user_id: Uuid, provider_user_id: i64, access: &str) -> UpstreamAccount {
	let now = Utc::now();
	UpstreamAccount {
		id: Uuid::now_v7(),
		user_id,
		provider: "github".to_owned(),
		provider_user_id: provider_user_id.to_string(),
		email: None,
		display_name: None,
		picture: None,
		access_token: access.to_owned(),
		refresh_token: Some("ghr_upstream_refresh".to_owned()),
		token_expires_at: now + Duration::hours(8),
		scopes: Vec::new(),
		created_at: now,
		updated_at: now,
	}
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_06_tokens_are_encrypted_in_the_database(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let user = insert_user(100, "encrypted", false).await;
	let storage = storage();

	// Act
	storage
		.store_tokens(
			user.id,
			&tokens("ghu_plain_access", Some("ghr_plain_refresh")),
		)
		.await
		.unwrap();

	// Assert
	let rows = StoredAccount::objects().all().all().await.unwrap();
	assert_eq!(rows.len(), 1);
	let access = &rows[0].encrypted_access_token;
	let refresh = rows[0].encrypted_refresh_token.as_deref().unwrap();
	assert!(access.starts_with("v1.primary."));
	assert!(refresh.starts_with("v1.primary."));
	assert!(!access.contains("ghu_plain_access"));
	assert!(!refresh.contains("ghr_plain_refresh"));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_06_explicit_load_returns_the_stored_tokens_and_expiries(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(101, "load", false).await;
	let storage = storage();
	let stored = tokens("ghu_access", Some("ghr_refresh"));

	// Act
	storage.store_tokens(user.id, &stored).await.unwrap();
	let loaded = storage.load_tokens(user.id).await.unwrap().unwrap();

	// Assert
	assert_eq!(loaded.access_token.expose_secret(), "ghu_access");
	assert_eq!(loaded.refresh_token.unwrap().expose_secret(), "ghr_refresh");
	assert_eq!(
		millis(loaded.access_token_expires_at),
		millis(stored.access_token_expires_at)
	);
	assert_eq!(
		loaded.refresh_token_expires_at.map(millis),
		stored.refresh_token_expires_at.map(millis)
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_06_ordinary_reads_never_return_a_token(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let user = insert_user(102, "ordinary", false).await;
	let storage = storage();
	storage
		.store_tokens(user.id, &tokens("ghu_access", Some("ghr_refresh")))
		.await
		.unwrap();

	// Act
	let by_identity = storage
		.find_by_provider_and_uid("github", "102")
		.await
		.unwrap()
		.unwrap();
	let by_user = storage.find_by_user(user.id).await.unwrap();

	// Assert
	assert_eq!(by_identity.access_token, "");
	assert_eq!(by_identity.refresh_token, None);
	assert_eq!(by_identity.provider, "github");
	assert_eq!(by_identity.provider_user_id, "102");
	assert_eq!(by_identity.user_id, user.id);
	assert_eq!(by_user.len(), 1);
	assert_eq!(by_user[0].access_token, "");
	assert_eq!(by_user[0].refresh_token, None);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_06_a_wrong_key_fails_loading_instead_of_returning_garbage(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(103, "wrongkey", false).await;
	storage()
		.store_tokens(user.id, &tokens("ghu_access", None))
		.await
		.unwrap();
	let other = OrmSocialAccountStorage::new(keyring(OTHER_TEST_KEY));

	// Act
	let result = other.load_tokens(user.id).await;

	// Assert
	assert!(matches!(
		result,
		Err(ProviderTokenError::Crypto(TokenCryptoError::Decrypt))
	));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_06_ciphertext_copied_to_another_users_row_does_not_decrypt(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let victim = insert_user(104, "victim", false).await;
	let attacker = insert_user(105, "attacker", false).await;
	let storage = storage();
	storage
		.store_tokens(victim.id, &tokens("ghu_victim_access", None))
		.await
		.unwrap();
	storage
		.store_tokens(attacker.id, &tokens("ghu_attacker_access", None))
		.await
		.unwrap();
	let victim_row = StoredAccount::objects()
		.filter(StoredAccount::field_user_id().eq(victim.id))
		.first()
		.await
		.unwrap()
		.unwrap();
	StoredAccount::objects()
		.filter(StoredAccount::field_user_id().eq(attacker.id))
		.update_fields([StoredAccount::field_encrypted_access_token()
			.assign(victim_row.encrypted_access_token.clone())])
		.await
		.unwrap();

	// Act
	let result = storage.load_tokens(attacker.id).await;

	// Assert
	assert!(matches!(
		result,
		Err(ProviderTokenError::Crypto(TokenCryptoError::Decrypt))
	));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_06_a_rotated_token_pair_replaces_both_tokens_and_both_expiries(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(106, "rotate", false).await;
	let storage = storage();
	storage
		.store_tokens(user.id, &tokens("access-1", Some("refresh-1")))
		.await
		.unwrap();
	let rotated = tokens("access-2", Some("refresh-2"));

	// Act
	storage.store_tokens(user.id, &rotated).await.unwrap();
	let loaded = storage.load_tokens(user.id).await.unwrap().unwrap();

	// Assert
	assert_eq!(loaded.access_token.expose_secret(), "access-2");
	assert_eq!(loaded.refresh_token.unwrap().expose_secret(), "refresh-2");
	assert_eq!(
		loaded.refresh_token_expires_at.map(millis),
		rotated.refresh_token_expires_at.map(millis)
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_06_tokens_of_an_unknown_user_are_not_stored(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;

	// Act
	let result = storage()
		.store_tokens(Uuid::now_v7(), &tokens("ghu_orphan", None))
		.await;

	// Assert
	assert!(matches!(result, Err(ProviderTokenError::UserNotFound)));
	assert_eq!(StoredAccount::objects().all().all().await.unwrap().len(), 0);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_06_trait_create_encrypts_the_tokens_it_receives(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let user = insert_user(107, "create", false).await;
	let storage = storage();

	// Act
	let created = storage
		.create(upstream_account(user.id, 107, "ghu_via_trait"))
		.await
		.unwrap();

	// Assert
	assert_eq!(created.access_token, "");
	let loaded = storage.load_tokens(user.id).await.unwrap().unwrap();
	assert_eq!(loaded.access_token.expose_secret(), "ghu_via_trait");
	assert_eq!(
		loaded.refresh_token.unwrap().expose_secret(),
		"ghr_upstream_refresh"
	);
	let row = StoredAccount::objects()
		.all()
		.all()
		.await
		.unwrap()
		.remove(0);
	assert!(!row.encrypted_access_token.contains("ghu_via_trait"));
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_03_trait_create_rejects_an_identity_that_is_not_the_users(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(108, "owner", false).await;
	let storage = storage();

	// Act
	let wrong_identity = storage
		.create(upstream_account(user.id, 999, "ghu_x"))
		.await;
	let mut other_provider = upstream_account(user.id, 108, "ghu_x");
	other_provider.provider = "gitlab".to_owned();
	let wrong_provider = storage.create(other_provider).await;

	// Assert
	assert!(matches!(wrong_identity, Err(SocialAuthError::Storage(_))));
	assert!(matches!(wrong_provider, Err(SocialAuthError::Storage(_))));
	assert_eq!(StoredAccount::objects().all().all().await.unwrap().len(), 0);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_03_trait_create_rejects_a_second_account_for_one_user(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(109, "once", false).await;
	let storage = storage();
	storage
		.create(upstream_account(user.id, 109, "ghu_first"))
		.await
		.unwrap();

	// Act
	let second = storage
		.create(upstream_account(user.id, 109, "ghu_second"))
		.await;

	// Assert
	assert!(matches!(second, Err(SocialAuthError::Storage(_))));
	let loaded = storage.load_tokens(user.id).await.unwrap().unwrap();
	assert_eq!(loaded.access_token.expose_secret(), "ghu_first");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_06_updating_with_a_tokenless_record_keeps_the_stored_tokens(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(110, "roundtrip", false).await;
	let storage = storage();
	storage
		.store_tokens(user.id, &tokens("ghu_kept", Some("ghr_kept")))
		.await
		.unwrap();
	let tokenless = storage.find_by_user(user.id).await.unwrap().remove(0);

	// Act
	storage.update(tokenless).await.unwrap();

	// Assert
	let loaded = storage.load_tokens(user.id).await.unwrap().unwrap();
	assert_eq!(loaded.access_token.expose_secret(), "ghu_kept");
	assert_eq!(loaded.refresh_token.unwrap().expose_secret(), "ghr_kept");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_06_updating_with_a_new_token_replaces_the_stored_one(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let user = insert_user(111, "refresh", false).await;
	let storage = storage();
	let created = storage
		.create(upstream_account(user.id, 111, "ghu_old"))
		.await
		.unwrap();
	let mut refreshed = upstream_account(user.id, 111, "ghu_new");
	refreshed.id = created.id;

	// Act
	storage.update(refreshed).await.unwrap();

	// Assert
	let loaded = storage.load_tokens(user.id).await.unwrap().unwrap();
	assert_eq!(loaded.access_token.expose_secret(), "ghu_new");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_06_delete_removes_the_tokens_and_a_missing_account_is_an_error(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(112, "delete", false).await;
	let storage = storage();
	let created = storage
		.create(upstream_account(user.id, 112, "ghu_gone"))
		.await
		.unwrap();

	// Act
	storage.delete(created.id).await.unwrap();
	let again = storage.delete(created.id).await;

	// Assert
	assert!(matches!(&again, Err(SocialAuthError::Storage(message))
			if *message == format!("Social account not found: {}", created.id)));
	assert!(storage.load_tokens(user.id).await.unwrap().is_none());
	assert!(storage.find_by_user(user.id).await.unwrap().is_empty());
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_03_deleting_a_user_removes_the_tokens(#[future] database: TestDatabase) {
	// Arrange
	let _db = database.await;
	let user = insert_user(113, "cascade", false).await;
	let storage = storage();
	storage
		.store_tokens(user.id, &tokens("ghu_cascade", None))
		.await
		.unwrap();

	// Act
	crate::apps::accounts::models::User::objects()
		.delete(user.id)
		.await
		.unwrap();

	// Assert
	assert_eq!(StoredAccount::objects().all().all().await.unwrap().len(), 0);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_03_trait_update_rejects_an_identity_that_is_not_the_users(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(114, "update-owner", false).await;
	let storage = storage();
	let created = storage
		.create(upstream_account(user.id, 114, "ghu_original"))
		.await
		.unwrap();
	let mut forged = upstream_account(user.id, 999, "ghu_forged");
	forged.id = created.id;

	// Act
	let result = storage.update(forged).await;

	// Assert
	assert!(matches!(
		&result,
		Err(SocialAuthError::Storage(message))
			if message == "provider user id does not match the user's GitHub identity"
	));
	let loaded = storage.load_tokens(user.id).await.unwrap().unwrap();
	assert_eq!(loaded.access_token.expose_secret(), "ghu_original");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_03_trait_update_rejects_a_record_that_belongs_to_another_user(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let owner = insert_user(115, "row-owner", false).await;
	let other = insert_user(116, "other-user", false).await;
	let storage = storage();
	let owners_account = storage
		.create(upstream_account(owner.id, 115, "ghu_owner"))
		.await
		.unwrap();
	storage
		.create(upstream_account(other.id, 116, "ghu_other"))
		.await
		.unwrap();
	// A consistent identity for `other`, but the id of the owner's row.
	let mut crossed = upstream_account(other.id, 116, "ghu_crossed");
	crossed.id = owners_account.id;

	// Act
	let result = storage.update(crossed).await;

	// Assert
	assert!(matches!(
		&result,
		Err(SocialAuthError::Storage(message))
			if *message == format!("Social account not found: {}", owners_account.id)
	));
	let owner_tokens = storage.load_tokens(owner.id).await.unwrap().unwrap();
	let other_tokens = storage.load_tokens(other.id).await.unwrap().unwrap();
	assert_eq!(owner_tokens.access_token.expose_secret(), "ghu_owner");
	assert_eq!(other_tokens.access_token.expose_secret(), "ghu_other");
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_06_trait_update_keeps_the_stored_refresh_expiry_for_a_refresh_token(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(117, "expiry", false).await;
	let storage = storage();
	let stored = tokens("ghu_old", Some("ghr_old"));
	storage.store_tokens(user.id, &stored).await.unwrap();
	let row_id = storage.find_by_user(user.id).await.unwrap().remove(0).id;
	let mut refreshed = upstream_account(user.id, 117, "ghu_new");
	refreshed.id = row_id;

	// Act
	storage.update(refreshed).await.unwrap();

	// Assert
	let loaded = storage.load_tokens(user.id).await.unwrap().unwrap();
	assert_eq!(loaded.access_token.expose_secret(), "ghu_new");
	assert_eq!(
		loaded.refresh_token.unwrap().expose_secret(),
		"ghr_upstream_refresh"
	);
	assert_eq!(
		loaded.refresh_token_expires_at.map(millis),
		stored.refresh_token_expires_at.map(millis)
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn sr_06_trait_update_without_a_refresh_token_clears_the_refresh_expiry(
	#[future] database: TestDatabase,
) {
	// Arrange
	let _db = database.await;
	let user = insert_user(118, "no-refresh", false).await;
	let storage = storage();
	storage
		.store_tokens(user.id, &tokens("ghu_old", Some("ghr_old")))
		.await
		.unwrap();
	let row_id = storage.find_by_user(user.id).await.unwrap().remove(0).id;
	let mut without_refresh = upstream_account(user.id, 118, "ghu_new");
	without_refresh.id = row_id;
	without_refresh.refresh_token = None;

	// Act
	storage.update(without_refresh).await.unwrap();

	// Assert
	let loaded = storage.load_tokens(user.id).await.unwrap().unwrap();
	assert!(loaded.refresh_token.is_none());
	assert_eq!(loaded.refresh_token_expires_at, None);
}
