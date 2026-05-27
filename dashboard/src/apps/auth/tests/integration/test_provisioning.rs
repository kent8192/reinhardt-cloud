//! Integration tests for the shared Personal Org provisioning service.
//!
//! Verifies that `provision_personal_organization` (refs #435) handles
//! both the happy path and the slug-collision retry branch correctly:
//! the retry must succeed by appending a uuid suffix to the original
//! slug and the resulting Organization must record the registering user
//! as `created_by`.

#[cfg(test)]
mod tests {
	use chrono::Utc;
	use reinhardt::BaseUser;
	use reinhardt::db::orm::Model;
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serial_test::serial;
	use std::sync::Arc;

	use crate::apps::auth::models::User;
	use crate::apps::auth::services::registration::provision_personal_organization;
	use crate::apps::organizations::models::{Organization, OrganizationMembership};
	use crate::apps::organizations::roles::sanitize_username_to_slug;
	use reinhardt::ServerRouter;

	#[fixture]
	async fn db() -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		Arc<ServerRouter>,
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

	/// Helper: insert a `User` row directly via ORM, bypassing the register
	/// endpoint (no email verification, no rollback semantics).
	async fn create_user(username: &str, email: &str) -> User {
		let mut user = User::new(
			username.to_string(),
			email.to_lowercase(),
			String::new(),
			String::new(),
			None,
			true,
			false,
			false,
		);
		user.set_password("test-password")
			.expect("Password hashing failed");
		User::objects()
			.create(&user)
			.await
			.expect("Failed to create user")
	}

	/// Happy path: derived slug is unused, so the first INSERT succeeds and
	/// the resulting Organization records the user as `created_by`.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_provision_personal_organization_happy_path(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<ServerRouter>,
		),
	) {
		// Arrange
		let (_container, _conn, _client, _urls) = db.await;
		let user = create_user("happypath", "happypath@example.com").await;
		let expected_slug = sanitize_username_to_slug(&user.username);

		// Act
		provision_personal_organization(&user)
			.await
			.expect("provision should succeed on the happy path");

		// Assert -- exactly one Organization with the derived slug, owned by `user`
		let org = Organization::objects()
			.filter(Organization::field_slug().eq(expected_slug.clone()))
			.first()
			.await
			.expect("query Organization by slug")
			.expect("Organization should exist after provisioning");
		assert_eq!(org.created_by, user.id, "created_by must equal user.id");
		assert_eq!(org.slug, expected_slug);

		// Assert -- exactly one Owner membership wiring user to org
		let membership = OrganizationMembership::objects()
			.filter(OrganizationMembership::field_user_id().eq(user.id.to_string()))
			.first()
			.await
			.expect("query membership")
			.expect("Owner membership should exist");
		assert_eq!(
			membership.organization_id,
			org.id.expect("created Organization has id"),
		);
		assert_eq!(membership.role, "owner");
	}

	/// Retry branch: a pre-existing Organization holds the slug the user's
	/// username would derive to, forcing the first INSERT to fail with a
	/// unique-violation. The provisioning service must retry once with a
	/// uuid-suffixed slug, succeed, and the resulting Organization must
	/// still record the registering user as `created_by`.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_provision_personal_organization_retries_on_slug_collision(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<ServerRouter>,
		),
	) {
		// Arrange -- pre-occupy the slug the new user's username will derive to
		let (_container, _conn, _client, _urls) = db.await;
		let squatter = create_user("squatter", "squatter@example.com").await;
		let collide_slug = "collide".to_string();
		let now = Utc::now();
		Organization::objects()
			.create(&Organization {
				id: None,
				slug: collide_slug.clone(),
				name: "pre-existing".to_string(),
				created_by: squatter.id,
				created_at: now,
				updated_at: now,
			})
			.await
			.expect("seed Organization with colliding slug");

		// Sanity check: the username's derived slug equals the squatted slug
		let new_user = create_user("collide", "collide@example.com").await;
		assert_eq!(
			sanitize_username_to_slug(&new_user.username),
			collide_slug,
			"test setup invariant: derived slug must collide",
		);

		// Act
		provision_personal_organization(&new_user)
			.await
			.expect("provision should succeed via retry branch");

		// Assert -- the new user's Organization exists with a uuid-suffixed slug
		// and records `new_user.id` as creator (NOT `squatter.id`)
		let new_org = Organization::objects()
			.filter(Organization::field_created_by().eq(new_user.id.to_string()))
			.first()
			.await
			.expect("query Organization by created_by")
			.expect("Organization for new_user should exist after retry");
		assert_eq!(
			new_org.created_by, new_user.id,
			"retry path must still attribute creation to the registering user",
		);
		assert_ne!(
			new_org.slug, collide_slug,
			"retry path must not reuse the squatted slug",
		);
		assert!(
			new_org.slug.starts_with(&format!("{collide_slug}-")),
			"retry slug must be `<original>-<6-char-suffix>`; got: {}",
			new_org.slug,
		);
		// 6-char hex suffix → "collide-" + 6 chars = 14 chars total
		assert_eq!(
			new_org.slug.len(),
			collide_slug.len() + 1 + 6,
			"retry suffix must be exactly 6 chars; got slug: {}",
			new_org.slug,
		);
	}
}
