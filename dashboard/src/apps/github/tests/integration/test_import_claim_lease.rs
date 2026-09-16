//! Database-backed import claim lease interleaving tests.

#![cfg(test)]

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use reinhardt::Model;
use reinhardt::test::fixtures::{
	ContainerAsync, GenericImage, MigrationDatabase, postgres_with_migrations_from_dir,
};
use rstest::{fixture, rstest};
use serial_test::serial;

use crate::apps::auth::models::User;
use crate::apps::github::models::{GitHubInstallation, GitHubRepository};
use crate::apps::github::server_fn::{recover_github_import_claim, renew_github_import_claim};
use crate::apps::organizations::models::Organization;

#[fixture]
async fn claimed_repository() -> (
	ContainerAsync<GenericImage>,
	MigrationDatabase,
	i64,
	DateTime<Utc>,
) {
	let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
	let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
		.await
		.expect("start PostgreSQL with migrations");
	let mut conn_handle = *conn;
	let user = User::objects()
		.create_with_conn(
			&mut conn_handle,
			&User::build()
				.username("claim-owner".to_string())
				.email("claim-owner@example.com".to_string())
				.first_name(String::new())
				.last_name(String::new())
				.password_hash(None)
				.is_active(true)
				.is_staff(false)
				.is_superuser(false)
				.finish(),
		)
		.await
		.expect("create claim owner");
	let now = Utc::now();
	let organization = Organization::objects()
		.create_with_conn(
			&mut conn_handle,
			&Organization {
				id: None,
				slug: "claim-owner".to_string(),
				name: "Claim Owner".to_string(),
				created_by: user.id,
				created_at: now,
				updated_at: now,
			},
		)
		.await
		.expect("create claim organization");
	let installation = GitHubInstallation::objects()
		.create_with_conn(
			&mut conn_handle,
			&GitHubInstallation::build()
				.organization(organization.id.expect("organization id"))
				.installation_id(981_001)
				.account_id(981_002)
				.account_login("claim-owner".to_string())
				.account_type("Organization".to_string())
				.status("active".to_string())
				.finish(),
		)
		.await
		.expect("create GitHub installation");
	let claimed_at = Utc
		.with_ymd_and_hms(2026, 8, 28, 0, 0, 0)
		.single()
		.expect("valid claim timestamp");
	let repository = GitHubRepository::objects()
		.create_with_conn(
			&mut conn_handle,
			&GitHubRepository::build()
				.installation(installation.id.expect("installation id"))
				.github_repository_id(981_003)
				.full_name("claim-owner/repository".to_string())
				.owner_login("claim-owner".to_string())
				.name("repository".to_string())
				.default_branch("main".to_string())
				.private(false)
				.selected(true)
				.import_claimed_at(Some(claimed_at))
				.finish(),
		)
		.await
		.expect("create claimed GitHub repository");
	let repository_id = repository.id.expect("repository id");
	GitHubRepository::objects()
		.filter(GitHubRepository::field_id().eq(repository_id))
		.update_fields_with_conn(
			&mut conn_handle,
			[GitHubRepository::field_updated_at().assign(claimed_at)],
		)
		.await
		.expect("align claim update timestamp");

	(container, conn, repository_id, claimed_at)
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn heartbeat_winning_the_cas_prevents_stale_recovery(
	#[future] claimed_repository: (
		ContainerAsync<GenericImage>,
		MigrationDatabase,
		i64,
		DateTime<Utc>,
	),
) {
	// Arrange
	let (_container, _conn, repository_id, claimed_at) = claimed_repository.await;
	let renewed_at = claimed_at + TimeDelta::try_minutes(10).expect("valid duration");

	// Act
	let renewed = renew_github_import_claim(repository_id, claimed_at, renewed_at)
		.await
		.expect("renew claim");
	let recovered = recover_github_import_claim(repository_id, Some(claimed_at), claimed_at)
		.await
		.expect("attempt stale recovery");

	// Assert
	assert_eq!(renewed, 1);
	assert_eq!(recovered, 0);
	let repository = GitHubRepository::objects()
		.filter(GitHubRepository::field_id().eq(repository_id))
		.first()
		.await
		.expect("load repository")
		.expect("repository exists");
	assert!(repository.selected);
	assert_eq!(repository.import_claimed_at, Some(renewed_at));
	assert_eq!(repository.updated_at, renewed_at);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn stale_recovery_winning_the_cas_prevents_heartbeat_renewal(
	#[future] claimed_repository: (
		ContainerAsync<GenericImage>,
		MigrationDatabase,
		i64,
		DateTime<Utc>,
	),
) {
	// Arrange
	let (_container, _conn, repository_id, claimed_at) = claimed_repository.await;
	let renewed_at = claimed_at + TimeDelta::try_minutes(10).expect("valid duration");

	// Act
	let recovered = recover_github_import_claim(repository_id, Some(claimed_at), claimed_at)
		.await
		.expect("recover stale claim");
	let renewed = renew_github_import_claim(repository_id, claimed_at, renewed_at)
		.await
		.expect("attempt claim renewal");

	// Assert
	assert_eq!(recovered, 1);
	assert_eq!(renewed, 0);
	let repository = GitHubRepository::objects()
		.filter(GitHubRepository::field_id().eq(repository_id))
		.first()
		.await
		.expect("load repository")
		.expect("repository exists");
	assert!(!repository.selected);
	assert_eq!(repository.import_claimed_at, None);
}
