//! Role-based access control tests for cluster API endpoints.
//!
//! Verifies that the `require_permission_for_org` guard correctly enforces
//! the permission matrix introduced by issue #417 across all cluster
//! endpoints. Each role-action combination exercises the boundary between
//! 200 / 201 / 204 / 403.

#[cfg(test)]
mod tests {
	use reinhardt::middleware::session::AsyncSessionBackend;
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serde_json::json;
	use serial_test::serial;
	use std::sync::Arc;

	use crate::apps::organizations::roles::MembershipRole;
	use reinhardt::UrlReverser;

	use crate::config::test_helpers::{
		force_login_user_with_org, session_backend, set_membership_role,
	};

	type DbFixture = (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		Arc<UrlReverser>,
		Arc<dyn AsyncSessionBackend>,
	);

	#[fixture]
	async fn db(session_backend: Arc<dyn AsyncSessionBackend>) -> DbFixture {
		// Start the TestContainers database first so that build_test_app() can
		// register the DatabaseConnection in the DI singleton scope. Fixes #459.
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		let (client, urls) = crate::config::test_helpers::build_test_app();
		(container, conn, client, urls, session_backend)
	}

	/// Helper: POST a cluster and return its primary key.
	async fn create_cluster_returning_id(client: &APIClient, org_slug: &str, name: &str) -> i64 {
		let data = json!({
			"name": name,
			"api_url": "https://k8s.example.com:6443",
		});
		let resp = client
			.post(&format!("/api/orgs/{org_slug}/clusters/"), &data, "json")
			.await
			.expect("Create cluster request failed");
		assert_eq!(resp.status_code(), 201, "Owner should be allowed to create");
		let body: serde_json::Value = resp.json().expect("Failed to parse JSON");
		body["id"].as_i64().expect("Created cluster missing id")
	}

	// =============================================================
	// Viewer (read-only) — should be able to list and retrieve, but
	// must be rejected with 403 on any write action.
	// =============================================================

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_viewer_can_list_clusters(#[future] db: DbFixture) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		let (user, org) = force_login_user_with_org(
			&client,
			&conn,
			&backend,
			"viewer_user",
			"viewer@example.com",
		)
		.await;
		let slug = &org.slug;
		// Default role is Owner (assigned in provision); demote to Viewer.
		set_membership_role(&conn, &user, MembershipRole::Viewer).await;

		// Act
		let resp = client
			.get(&format!("/api/orgs/{slug}/clusters/"))
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(resp.status_code(), 200);
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_viewer_cannot_create_cluster(#[future] db: DbFixture) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		let (user, org) = force_login_user_with_org(
			&client,
			&conn,
			&backend,
			"viewer_create",
			"viewer-create@example.com",
		)
		.await;
		let slug = &org.slug;
		set_membership_role(&conn, &user, MembershipRole::Viewer).await;

		// Act
		let data = json!({
			"name": "viewer-cluster",
			"api_url": "https://k8s.example.com:6443",
		});
		let resp = client
			.post(&format!("/api/orgs/{slug}/clusters/"), &data, "json")
			.await
			.expect("Create cluster request failed");

		// Assert
		assert_eq!(
			resp.status_code(),
			403,
			"Viewer must be denied with 403 for ClusterCreate"
		);
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_viewer_cannot_delete_cluster(#[future] db: DbFixture) {
		// Arrange — Owner creates cluster first, then we demote to Viewer.
		let (_container, conn, client, _urls, backend) = db.await;
		let (user, org) = force_login_user_with_org(
			&client,
			&conn,
			&backend,
			"viewer_del",
			"viewer-del@example.com",
		)
		.await;
		let slug = &org.slug;
		let cluster_id = create_cluster_returning_id(&client, slug, "demote-target").await;
		set_membership_role(&conn, &user, MembershipRole::Viewer).await;

		// Act
		let resp = client
			.delete(&format!("/api/orgs/{slug}/clusters/{cluster_id}/"))
			.await
			.expect("Delete cluster request failed");

		// Assert
		assert_eq!(
			resp.status_code(),
			403,
			"Viewer must be denied with 403 for ClusterDelete"
		);
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_viewer_cannot_update_cluster(#[future] db: DbFixture) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		let (user, org) = force_login_user_with_org(
			&client,
			&conn,
			&backend,
			"viewer_upd",
			"viewer-upd@example.com",
		)
		.await;
		let slug = &org.slug;
		let cluster_id = create_cluster_returning_id(&client, slug, "demote-update").await;
		set_membership_role(&conn, &user, MembershipRole::Viewer).await;

		// Act
		let data = json!({ "name": "renamed" });
		let resp = client
			.patch(
				&format!("/api/orgs/{slug}/clusters/{cluster_id}/"),
				&data,
				"json",
			)
			.await
			.expect("Update cluster request failed");

		// Assert
		assert_eq!(
			resp.status_code(),
			403,
			"Viewer must be denied with 403 for ClusterUpdate"
		);
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_viewer_can_retrieve_cluster(#[future] db: DbFixture) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		let (user, org) = force_login_user_with_org(
			&client,
			&conn,
			&backend,
			"viewer_get",
			"viewer-get@example.com",
		)
		.await;
		let slug = &org.slug;
		let cluster_id = create_cluster_returning_id(&client, slug, "viewer-readable").await;
		set_membership_role(&conn, &user, MembershipRole::Viewer).await;

		// Act
		let resp = client
			.get(&format!("/api/orgs/{slug}/clusters/{cluster_id}/"))
			.await
			.expect("Retrieve cluster request failed");

		// Assert
		assert_eq!(
			resp.status_code(),
			200,
			"Viewer must be allowed to read cluster metadata"
		);
	}

	// =============================================================
	// Developer / Owner — full CRUD on clusters in their org.
	// =============================================================

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_developer_can_create_cluster(#[future] db: DbFixture) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		let (user, org) = force_login_user_with_org(
			&client,
			&conn,
			&backend,
			"dev_create",
			"dev-create@example.com",
		)
		.await;
		let slug = &org.slug;
		set_membership_role(&conn, &user, MembershipRole::Developer).await;

		// Act
		let data = json!({
			"name": "dev-cluster",
			"api_url": "https://k8s.example.com:6443",
		});
		let resp = client
			.post(&format!("/api/orgs/{slug}/clusters/"), &data, "json")
			.await
			.expect("Create cluster request failed");

		// Assert
		assert_eq!(resp.status_code(), 201);
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_owner_can_delete_cluster(#[future] db: DbFixture) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		let (_user, org) = force_login_user_with_org(
			&client,
			&conn,
			&backend,
			"owner_del",
			"owner-del@example.com",
		)
		.await;
		let slug = &org.slug;
		let cluster_id = create_cluster_returning_id(&client, slug, "owner-delete-me").await;

		// Act
		let resp = client
			.delete(&format!("/api/orgs/{slug}/clusters/{cluster_id}/"))
			.await
			.expect("Delete cluster request failed");

		// Assert
		assert_eq!(resp.status_code(), 204);
	}
}
