//! Integration tests for `link_or_create_user`.
//!
//! Covers the four-way decision tree from #428: already-linked,
//! authenticated link, email-verified email match, and new-user creation.
//! `email_verified == None` (the GitHub case) is also covered to confirm
//! the safe-by-default behavior of declining to merge into an existing
//! local account when the provider does not assert verification.

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use reinhardt::BaseUser;
	use reinhardt::db::orm::Model;
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::reinhardt_auth::social::core::claims::StandardClaims;
	use reinhardt::reinhardt_auth::social::storage::{
		InMemorySocialAccountStorage, SocialAccountStorage,
	};
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serial_test::serial;
	use std::collections::HashMap;

	use crate::apps::auth::models::User;
	use crate::apps::auth::services::oauth::linking::{LinkError, link_or_create_user};
	use reinhardt::UrlReverser;

	#[fixture]
	async fn db() -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		Arc<UrlReverser>,
	) {
		// Start TestContainers first so build_test_app() registers DatabaseConnection
		// in the DI scope. Fixes #459.
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		let (client, urls) = crate::config::test_helpers::build_test_app();
		(container, conn, client, urls)
	}

	fn github_claims(
		sub: &str,
		email: Option<&str>,
		email_verified: Option<bool>,
	) -> StandardClaims {
		let mut additional = HashMap::new();
		additional.insert(
			"login".to_string(),
			serde_json::Value::String(format!("user_{sub}")),
		);
		StandardClaims {
			sub: sub.to_string(),
			email: email.map(String::from),
			email_verified,
			name: Some(format!("User {sub}")),
			given_name: None,
			family_name: None,
			picture: None,
			locale: None,
			additional_claims: additional,
		}
	}

	async fn seed_user(username: &str, email: &str) -> uuid::Uuid {
		let mut user = User::build()
			.username(username.to_string())
			.email(email.to_lowercase())
			.first_name(String::new())
			.last_name(String::new())
			.password_hash(None)
			.is_active(true)
			.is_staff(false)
			.is_superuser(false)
			.finish();
		user.set_password("test-password-1234").unwrap();
		User::objects().create(&user).await.expect("seed user").id
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_already_linked_returns_existing_user(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange
		let (_c, _conn, _cli, _urls) = db.await;
		let user_id = seed_user("link_existing", "existing@example.com").await;
		let storage = InMemorySocialAccountStorage::new();
		let claims = github_claims("gh_99", Some("existing@example.com"), Some(true));
		// Pre-existing link
		link_or_create_user(&storage, "github", &claims, None)
			.await
			.expect("first call creates the link via path (c)");

		// Act — second call hits path (a)
		let user = link_or_create_user(&storage, "github", &claims, None)
			.await
			.expect("second call should return the linked user");

		// Assert
		assert_eq!(user.id, user_id);
		// No second link was created — find_by_provider returns just one.
		let links = storage.find_by_user(user_id).await.unwrap();
		assert_eq!(links.len(), 1);
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_authenticated_link_attaches_to_current_user(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange
		let (_c, _conn, _cli, _urls) = db.await;
		let user_id = seed_user("link_authed", "authed@example.com").await;
		let current = User::objects()
			.filter(User::field_id().eq(user_id.to_string()))
			.first()
			.await
			.unwrap()
			.unwrap();
		let storage = InMemorySocialAccountStorage::new();
		// Claims have a *different* email and unverified state — would fall
		// through to a new-user branch if not for the authenticated link.
		let claims = github_claims("gh_authed_42", Some("not-authed@example.com"), None);

		// Act
		let user = link_or_create_user(&storage, "github", &claims, Some(current))
			.await
			.expect("authed link should succeed");

		// Assert
		assert_eq!(user.id, user_id);
		let links = storage.find_by_user(user_id).await.unwrap();
		assert_eq!(links.len(), 1);
		assert_eq!(links[0].provider_user_id, "gh_authed_42");
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_email_verified_match_links_existing_user(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange
		let (_c, _conn, _cli, _urls) = db.await;
		let user_id = seed_user("link_emailmatch", "match@example.com").await;
		let storage = InMemorySocialAccountStorage::new();
		let claims = github_claims("gh_match_1", Some("match@example.com"), Some(true));

		// Act
		let user = link_or_create_user(&storage, "github", &claims, None)
			.await
			.expect("email-match link should succeed");

		// Assert
		assert_eq!(user.id, user_id);
		let links = storage.find_by_user(user_id).await.unwrap();
		assert_eq!(links.len(), 1);
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_email_unverified_collision_returns_email_conflict(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange — an existing local user owns the email, and the OAuth
		// provider returns the same email but does NOT assert verification
		// (email_verified = None mirrors GitHub's surface).
		let (_c, _conn, _cli, _urls) = db.await;
		let existing_id = seed_user("link_unverified", "unverified@example.com").await;
		let storage = InMemorySocialAccountStorage::new();
		let claims = github_claims("gh_unv_1", Some("unverified@example.com"), None);

		// Act
		let result = link_or_create_user(&storage, "github", &claims, None).await;

		// Assert — must NOT silently auto-merge (path (c) requires
		// verified=true) and must NOT create a duplicate-email user. The
		// caller gets a structured EmailConflict error so the UI can
		// surface a "sign in with your existing account first" message.
		match result {
			Err(LinkError::EmailConflict { email, provider }) => {
				assert_eq!(email, "unverified@example.com");
				assert_eq!(provider, "github");
			}
			other => panic!("expected EmailConflict, got {other:?}"),
		}
		// Existing user is untouched.
		let links = storage.find_by_user(existing_id).await.unwrap();
		assert!(links.is_empty(), "existing user must not be auto-linked");
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_new_user_created_with_no_password(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange
		let (_c, _conn, _cli, _urls) = db.await;
		let storage = InMemorySocialAccountStorage::new();
		let claims = github_claims("gh_new_777", Some("brand-new@example.com"), Some(true));

		// Act
		let user = link_or_create_user(&storage, "github", &claims, None)
			.await
			.expect("new-user creation should succeed");

		// Assert
		assert!(user.password_hash.is_none(), "OAuth user has no password");
		assert!(user.is_active(), "new OAuth user is active by default");
		assert!(!user.is_staff);
		assert!(!user.is_superuser);
		assert_eq!(user.email, "brand-new@example.com");
		// Username is derived from the `login` additional claim and sanitized.
		assert_eq!(user.get_username(), "user_gh_new_777");
		let links = storage.find_by_user(user.id).await.unwrap();
		assert_eq!(links.len(), 1);
		assert_eq!(links[0].provider, "github");
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_username_collision_appends_suffix(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange — existing user already has the candidate username.
		let (_c, _conn, _cli, _urls) = db.await;
		seed_user("user_gh_collide", "existing-collide@example.com").await;
		let storage = InMemorySocialAccountStorage::new();
		let claims = github_claims("gh_collide", Some("oauth-collide@example.com"), Some(true));

		// Act
		let user = link_or_create_user(&storage, "github", &claims, None)
			.await
			.expect("new-user creation should succeed even on collision");

		// Assert
		assert_eq!(user.get_username(), "user_gh_collide_1");
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_email_verified_true_but_no_email_creates_new_user(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange — a provider that asserts `email_verified == true` but
		// returns no email at all (rare, but legal). Path (c) requires
		// BOTH the verified flag AND a present email; missing the email
		// must short-circuit (c) and fall through to (d) "new user".
		let (_c, _conn, _cli, _urls) = db.await;
		let storage = InMemorySocialAccountStorage::new();
		let claims = github_claims("gh_no_email_42", None, Some(true));

		// Act
		let user = link_or_create_user(&storage, "github", &claims, None)
			.await
			.expect("missing email with verified=true should still create a new user");

		// Assert — new user, no email, derived username from the `login`
		// additional claim.
		assert!(user.password_hash.is_none());
		assert_eq!(user.email, "");
		assert_eq!(user.get_username(), "user_gh_no_email_42");
		let links = storage.find_by_user(user.id).await.unwrap();
		assert_eq!(links.len(), 1);
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_username_falls_back_to_sub_when_no_login_or_name(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<UrlReverser>,
		),
	) {
		// Arrange — claims with no `login` additional claim and no
		// `name`. `display_name_from_claims` returns None and
		// `generate_unique_username` falls back to using `sub` (after
		// sanitization).
		let (_c, _conn, _cli, _urls) = db.await;
		let storage = InMemorySocialAccountStorage::new();
		let claims = StandardClaims {
			sub: "raw-sub-42".to_string(),
			email: Some("nameless@example.com".to_string()),
			email_verified: Some(true),
			name: None,
			given_name: None,
			family_name: None,
			picture: None,
			locale: None,
			additional_claims: HashMap::new(),
		};

		// Act
		let user = link_or_create_user(&storage, "github", &claims, None)
			.await
			.expect("creation should succeed using sub as username source");

		// Assert — `raw-sub-42` is allowed verbatim by the username
		// sanitizer (alnum, '-', '_', '.' are kept).
		assert_eq!(user.get_username(), "raw-sub-42");
	}
}
