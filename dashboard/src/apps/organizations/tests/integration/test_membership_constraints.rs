//! PostgreSQL integration tests for organization constraints.

use chrono::Utc;
use reinhardt::db::orm::Model;
use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
use reinhardt::test::fixtures::{ContainerAsync, GenericImage, MigrationDatabase};
use rstest::{fixture, rstest};
use serial_test::serial;
use uuid::Uuid;

use crate::apps::auth::models::User;
use crate::apps::organizations::models::{Organization, OrganizationMembership};

#[fixture]
async fn db() -> (ContainerAsync<GenericImage>, MigrationDatabase) {
	let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
	postgres_with_migrations_from_dir(&migrations_dir)
		.await
		.expect("start PostgreSQL with dashboard migrations")
}

#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn membership_role_check_rejects_invalid_database_value(
	#[future] db: (ContainerAsync<GenericImage>, MigrationDatabase),
) {
	// Arrange
	let (_container, conn) = db.await;
	let mut conn_handle = *conn;
	let user_id = Uuid::new_v4();
	let user = User::objects()
		.create_with_conn(
			&mut conn_handle,
			&User::build()
				.id(user_id)
				.username("membership-check".to_string())
				.email("membership-check@example.test".to_string())
				.first_name(String::new())
				.last_name(String::new())
				.password_hash(None)
				.is_active(true)
				.is_staff(false)
				.is_superuser(false)
				.finish(),
		)
		.await
		.expect("create user");
	let now = Utc::now();
	let organization = Organization::objects()
		.create_with_conn(
			&mut conn_handle,
			&Organization {
				id: None,
				slug: "membership-check".to_string(),
				name: "Membership Check".to_string(),
				created_by: user.id,
				created_at: now,
				updated_at: now,
			},
		)
		.await
		.expect("create organization");
	let organization_id = organization.id.expect("created organization ID");
	OrganizationMembership::objects()
		.create_with_conn(
			&mut conn_handle,
			&OrganizationMembership::build()
				.organization(organization_id)
				.user(user.id)
				.role("owner".to_string())
				.finish(),
		)
		.await
		.expect("create valid membership");
	let invalid_user_id = Uuid::new_v4();
	let invalid_user = User::objects()
		.create_with_conn(
			&mut conn_handle,
			&User::build()
				.id(invalid_user_id)
				.username("membership-check-invalid".to_string())
				.email("membership-check-invalid@example.test".to_string())
				.first_name(String::new())
				.last_name(String::new())
				.password_hash(None)
				.is_active(true)
				.is_staff(false)
				.is_superuser(false)
				.finish(),
		)
		.await
		.expect("create user for invalid membership");

	// Act
	let invalid = OrganizationMembership::objects()
		.create_with_conn(
			&mut conn_handle,
			&OrganizationMembership::build()
				.organization(organization_id)
				.user(invalid_user.id)
				.role("root".to_string())
				.finish(),
		)
		.await;
	let memberships = OrganizationMembership::objects()
		.filter(OrganizationMembership::field_organization_id().eq(organization_id))
		.all_with_db(&mut conn_handle)
		.await
		.expect("query memberships");

	// Assert
	assert!(
		invalid.is_err(),
		"database must reject role outside role_check"
	);
	assert_eq!(memberships.len(), 1);
	assert_eq!(memberships[0].role, "owner");
}
