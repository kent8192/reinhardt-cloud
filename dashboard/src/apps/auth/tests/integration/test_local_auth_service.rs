//! Local authentication reads UUID users through the real PostgreSQL ORM.
#![cfg(test)]
use reinhardt::db::orm::Model;
use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
use reinhardt_cloud_core::traits::AuthService;
use rstest::rstest;
use serial_test::serial;

use crate::apps::auth::{models::User, services::local_auth::LocalAuthService};

#[rstest]
#[tokio::test(flavor = "multi_thread")]
#[serial(database)]
async fn local_user_lookup_binds_a_uuid() {
	// Arrange
	let migrations = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
	let (_container, _connection) = postgres_with_migrations_from_dir(&migrations)
		.await
		.expect("disposable PostgreSQL migration fixture");
	let user = User::build()
		.username("uuid-lookup")
		.email("uuid-lookup@example.com")
		.first_name(String::new())
		.last_name(String::new())
		.password_hash(None)
		.is_active(true)
		.is_staff(false)
		.is_superuser(false)
		.finish();
	let saved = User::objects().create(&user).await.unwrap();

	// Act
	let found = LocalAuthService::new()
		.get_user_info(&saved.id.to_string())
		.await
		.expect("existing UUID user must be found");

	// Assert
	assert_eq!(found.id, saved.id);
	assert_eq!(found.username, saved.username);
	assert_eq!(found.email, saved.email);
	assert_eq!(found.password_hash, "");
}
