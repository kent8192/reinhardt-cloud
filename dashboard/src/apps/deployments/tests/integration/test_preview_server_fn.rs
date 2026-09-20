//! Preview server function ownership and display mapping tests.

#![cfg(test)]

use chrono::Utc;
use reinhardt::CurrentUser;
use reinhardt::Model;
use reinhardt::test::fixtures::{
	ContainerAsync, GenericImage, MigrationDatabase, postgres_with_migrations_from_dir,
};
use rstest::fixture;
use rstest::rstest;
use serial_test::serial;

use crate::apps::auth::models::User;
use crate::apps::clusters::models::Cluster;
use crate::apps::deployments::models::Deployment;
use crate::apps::deployments::server_fn::{
	ProjectSourceKind, delete_deployment_for_current_org, list_deployment_previews_for_current_org,
};
use crate::apps::github::models::{GitHubInstallation, GitHubProject, GitHubRepository};
use crate::apps::github::server_fn::list_github_project_previews_for_current_org;
use crate::apps::organizations::models::{Organization, OrganizationMembership};
use crate::apps::organizations::roles::MembershipRole;

#[fixture]
async fn db() -> (ContainerAsync<GenericImage>, MigrationDatabase) {
	let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
	postgres_with_migrations_from_dir(&migrations_dir)
		.await
		.expect("Failed to start PostgreSQL with migrations")
}

async fn create_user(conn: &MigrationDatabase, username: &str) -> User {
	let mut conn_handle = **conn;
	User::objects()
		.create_with_conn(
			&mut conn_handle,
			&User::build()
				.username(username.to_string())
				.email(format!("{username}@example.com"))
				.first_name(String::new())
				.last_name(String::new())
				.password_hash(None)
				.is_active(true)
				.is_staff(false)
				.is_superuser(false)
				.finish(),
		)
		.await
		.expect("create user")
}

async fn create_org(conn: &MigrationDatabase, creator: &User, slug: &str) -> Organization {
	let now = Utc::now();
	let mut conn_handle = **conn;
	Organization::objects()
		.create_with_conn(
			&mut conn_handle,
			&Organization {
				id: None,
				slug: slug.to_string(),
				name: slug.to_string(),
				created_by: creator.id,
				created_at: now,
				updated_at: now,
			},
		)
		.await
		.expect("create org")
}

async fn add_membership(
	conn: &MigrationDatabase,
	user: &User,
	org: &Organization,
	role: MembershipRole,
) {
	let mut conn_handle = **conn;
	OrganizationMembership::objects()
		.create_with_conn(
			&mut conn_handle,
			&OrganizationMembership::build()
				.organization(org.id.expect("created org has id"))
				.user(user.id)
				.role(role.as_db_str().to_string())
				.finish(),
		)
		.await
		.expect("create membership");
}

async fn create_cluster(conn: &MigrationDatabase, org: &Organization, name: &str) -> Cluster {
	let mut conn_handle = **conn;
	Cluster::objects()
		.create_with_conn(
			&mut conn_handle,
			&Cluster::build()
				.organization(org.id.expect("created org has id"))
				.name(name.to_string())
				.api_url(format!("https://{name}.k8s.example.com"))
				.is_active(true)
				.token_hash(None)
				.token_last_rotated_at(None)
				.finish(),
		)
		.await
		.expect("create cluster")
}

async fn create_deployment(
	conn: &MigrationDatabase,
	org: &Organization,
	cluster: &Cluster,
	project_name: &str,
	project_yaml: Option<String>,
) -> Deployment {
	let mut conn_handle = **conn;
	Deployment::objects()
		.create_with_conn(
			&mut conn_handle,
			&Deployment::build()
				.organization(org.id.expect("created org has id"))
				.project_name(project_name.to_string())
				.cluster(cluster.id.expect("created cluster has id"))
				.status("running".to_string())
				.image(format!("ghcr.io/example/{project_name}:latest"))
				.project_yaml(project_yaml)
				.finish(),
		)
		.await
		.expect("create deployment")
}

async fn create_github_project(
	conn: &MigrationDatabase,
	org: &Organization,
	deployment: &Deployment,
) -> GitHubProject {
	let mut conn_handle = **conn;
	let installation = GitHubInstallation::objects()
		.create_with_conn(
			&mut conn_handle,
			&GitHubInstallation::build()
				.organization(org.id.expect("created org has id"))
				.installation_id(721_001)
				.account_id(721_002)
				.account_login("kent8192".to_string())
				.account_type("Organization".to_string())
				.status("active".to_string())
				.finish(),
		)
		.await
		.expect("create github installation");
	let repository = GitHubRepository::objects()
		.create_with_conn(
			&mut conn_handle,
			&GitHubRepository::build()
				.installation(installation.id.expect("created installation has id"))
				.github_repository_id(721_003)
				.full_name("kent8192/reinhardt-cloud".to_string())
				.owner_login("kent8192".to_string())
				.name("reinhardt-cloud".to_string())
				.default_branch("main".to_string())
				.private(false)
				.selected(true)
				.import_claimed_at(None)
				.finish(),
		)
		.await
		.expect("create github repository");
	GitHubProject::objects()
		.create_with_conn(
			&mut conn_handle,
			&GitHubProject::build()
				.organization(org.id.expect("created org has id"))
				.repository(repository.id.expect("created repository has id"))
				.deployment(deployment.id.expect("created deployment has id"))
				.project_name("reinhardt-cloud".to_string())
				.production_branch("main".to_string())
				.status("imported".to_string())
				.finish(),
		)
		.await
		.expect("create github project")
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn deployment_preview_list_scopes_to_current_org_and_reports_row_errors(
	#[future] db: (ContainerAsync<GenericImage>, MigrationDatabase),
) {
	// Arrange
	let (_container, conn) = db.await;
	let user = create_user(&conn, "viewer").await;
	let other_user = create_user(&conn, "other").await;
	let org = create_org(&conn, &user, "viewer-org").await;
	let other_org = create_org(&conn, &other_user, "other-org").await;
	add_membership(&conn, &user, &org, MembershipRole::Viewer).await;
	add_membership(&conn, &other_user, &other_org, MembershipRole::Owner).await;
	let cluster = create_cluster(&conn, &org, "viewer-cluster").await;
	let other_cluster = create_cluster(&conn, &other_org, "other-cluster").await;
	let deployment = create_deployment(&conn, &org, &cluster, "api", None).await;
	create_deployment(&conn, &other_org, &other_cluster, "other-api", None).await;

	// Act
	let summaries = list_deployment_previews_for_current_org(CurrentUser(user))
		.await
		.expect("list deployment previews");

	// Assert
	assert_eq!(summaries.len(), 1);
	assert_eq!(
		summaries[0].deployment_id,
		deployment.id.expect("deployment id")
	);
	assert_eq!(summaries[0].project_name, "api");
	assert_eq!(summaries[0].display_name, "api");
	assert_eq!(summaries[0].source_kind, ProjectSourceKind::Manual);
	assert_eq!(
		summaries[0].preview_error.as_deref(),
		Some("Preview status is unavailable until cluster agent telemetry reports Project status")
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn github_preview_list_uses_repository_full_name_and_branch(
	#[future] db: (ContainerAsync<GenericImage>, MigrationDatabase),
) {
	// Arrange
	let (_container, conn) = db.await;
	let user = create_user(&conn, "github-viewer").await;
	let org = create_org(&conn, &user, "github-viewer-org").await;
	add_membership(&conn, &user, &org, MembershipRole::Viewer).await;
	let cluster = create_cluster(&conn, &org, "github-cluster").await;
	let deployment = create_deployment(&conn, &org, &cluster, "reinhardt-cloud", None).await;
	let github_project = create_github_project(&conn, &org, &deployment).await;

	// Act
	let summaries = list_github_project_previews_for_current_org(CurrentUser(user))
		.await
		.expect("list github previews");

	// Assert
	assert_eq!(summaries.len(), 1);
	assert_eq!(
		summaries[0].deployment_id,
		deployment.id.expect("deployment id")
	);
	assert_eq!(
		summaries[0].github_project_id, github_project.id,
		"github_project_id should preserve imported project identity"
	);
	assert_eq!(summaries[0].project_name, "reinhardt-cloud");
	assert_eq!(summaries[0].display_name, "kent8192/reinhardt-cloud");
	assert_eq!(summaries[0].production_branch.as_deref(), Some("main"));
	assert_eq!(summaries[0].source_kind, ProjectSourceKind::GitHub);
	assert_eq!(
		summaries[0].preview_error.as_deref(),
		Some("Preview status is unavailable until cluster agent telemetry reports Project status")
	);
}

#[rstest]
#[tokio::test]
#[serial(database)]
async fn deleting_github_deployment_releases_repository_import_claim(
	#[future] db: (ContainerAsync<GenericImage>, MigrationDatabase),
) {
	// Arrange
	let (_container, conn) = db.await;
	let user = create_user(&conn, "github-deleter").await;
	let org = create_org(&conn, &user, "github-deleter-org").await;
	add_membership(&conn, &user, &org, MembershipRole::Owner).await;
	let cluster = create_cluster(&conn, &org, "github-deleter-cluster").await;
	let deployment = create_deployment(&conn, &org, &cluster, "reinhardt-cloud", None).await;
	let github_project = create_github_project(&conn, &org, &deployment).await;
	let repository_id = github_project.repository_id();
	let claim_started_at = Utc::now();
	GitHubRepository::objects()
		.filter(GitHubRepository::field_id().eq(repository_id))
		.update_fields([
			GitHubRepository::field_selected().assign(true),
			GitHubRepository::field_import_claimed_at().assign(Some(claim_started_at)),
		])
		.await
		.expect("claim github repository");
	let database = reinhardt::db::orm::get_connection()
		.await
		.expect("get database connection");

	// Act
	delete_deployment_for_current_org(
		deployment.id.expect("deployment id").to_string(),
		CurrentUser(user),
		database,
	)
	.await
	.expect("delete deployment");

	// Assert
	assert!(
		Deployment::objects()
			.filter(Deployment::field_id().eq(deployment.id.expect("deployment id")))
			.first()
			.await
			.expect("query deleted deployment")
			.is_none()
	);
	assert!(
		GitHubProject::objects()
			.filter(GitHubProject::field_id().eq(github_project.id.expect("github project id")))
			.first()
			.await
			.expect("query deleted github project")
			.is_none()
	);
	let repository = GitHubRepository::objects()
		.filter(GitHubRepository::field_id().eq(repository_id))
		.first()
		.await
		.expect("query released github repository")
		.expect("github repository remains after project deletion");
	assert!(!repository.selected);
	assert_eq!(repository.import_claimed_at, None);
}
