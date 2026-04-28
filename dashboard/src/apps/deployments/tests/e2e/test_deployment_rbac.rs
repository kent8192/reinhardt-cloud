//! Role-based access control tests for deployment API endpoints.
//!
//! Verifies that the `require_permission` guard correctly enforces the
//! permission matrix introduced by issue #417 across deployment views.
//! The list/retrieve paths must accept Viewers; create/update/delete
//! must reject Viewers with 403.

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
	use crate::config::test_helpers::{
		ResolvedUrls, force_login_user, session_backend, set_membership_role, test_app,
	};

	type DbFixture = (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		ResolvedUrls,
		Arc<dyn AsyncSessionBackend>,
	);

	#[fixture]
	async fn db(
		test_app: (APIClient, ResolvedUrls),
		session_backend: Arc<dyn AsyncSessionBackend>,
	) -> DbFixture {
		let (client, urls) = test_app;
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		(container, conn, client, urls, session_backend)
	}

	/// Helper: create a cluster as the active Owner and return its id.
	async fn create_cluster(client: &APIClient) -> i64 {
		let data = json!({
			"name": "rbac-cluster",
			"api_url": "https://k8s.example.com:6443",
		});
		let resp = client
			.post("/api/clusters/", &data, "json")
			.await
			.expect("Create cluster failed");
		assert_eq!(resp.status_code(), 201);
		let body: serde_json::Value = resp.json().expect("Failed to parse cluster response");
		body["id"].as_i64().expect("cluster id")
	}

	/// Helper: create a deployment and return its id.
	async fn create_deployment(client: &APIClient, cluster_id: i64) -> i64 {
		let data = json!({
			"app_name": "rbac-app",
			"cluster_id": cluster_id,
			"image": "nginx:latest",
		});
		let resp = client
			.post("/api/deployments/", &data, "json")
			.await
			.expect("Create deployment failed");
		assert_eq!(resp.status_code(), 201);
		let body: serde_json::Value = resp.json().expect("Failed to parse deployment response");
		body["id"].as_i64().expect("deployment id")
	}

	// =============================================================
	// Viewer
	// =============================================================

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_viewer_can_list_deployments(#[future] db: DbFixture) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		let user = force_login_user(
			&client,
			&conn,
			&backend,
			"viewer_dep_list",
			"viewer-dep-list@example.com",
		)
		.await;
		set_membership_role(&conn, &user, MembershipRole::Viewer).await;

		// Act
		let resp = client
			.get("/api/deployments/")
			.await
			.expect("List deployments request failed");

		// Assert
		assert_eq!(resp.status_code(), 200);
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_viewer_cannot_create_deployment(#[future] db: DbFixture) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		let user = force_login_user(
			&client,
			&conn,
			&backend,
			"viewer_dep_create",
			"viewer-dep-create@example.com",
		)
		.await;
		// Owner privileges needed to create the cluster first.
		let cluster_id = create_cluster(&client).await;
		// Demote to Viewer before attempting the deployment create.
		set_membership_role(&conn, &user, MembershipRole::Viewer).await;

		// Act
		let data = json!({
			"app_name": "denied-app",
			"cluster_id": cluster_id,
			"image": "nginx:latest",
		});
		let resp = client
			.post("/api/deployments/", &data, "json")
			.await
			.expect("Create deployment request failed");

		// Assert
		assert_eq!(
			resp.status_code(),
			403,
			"Viewer must be denied with 403 for DeploymentCreate"
		);
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_viewer_cannot_delete_deployment(#[future] db: DbFixture) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		let user = force_login_user(
			&client,
			&conn,
			&backend,
			"viewer_dep_del",
			"viewer-dep-del@example.com",
		)
		.await;
		let cluster_id = create_cluster(&client).await;
		let deployment_id = create_deployment(&client, cluster_id).await;
		set_membership_role(&conn, &user, MembershipRole::Viewer).await;

		// Act
		let resp = client
			.delete(&format!("/api/deployments/{deployment_id}/"))
			.await
			.expect("Delete deployment request failed");

		// Assert
		assert_eq!(
			resp.status_code(),
			403,
			"Viewer must be denied with 403 for DeploymentDelete"
		);
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_viewer_cannot_update_deployment(#[future] db: DbFixture) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		let user = force_login_user(
			&client,
			&conn,
			&backend,
			"viewer_dep_upd",
			"viewer-dep-upd@example.com",
		)
		.await;
		let cluster_id = create_cluster(&client).await;
		let deployment_id = create_deployment(&client, cluster_id).await;
		set_membership_role(&conn, &user, MembershipRole::Viewer).await;

		// Act
		let data = json!({ "app_name": "renamed-app" });
		let resp = client
			.put(
				&format!("/api/deployments/{deployment_id}/"),
				&data,
				"json",
			)
			.await
			.expect("Update deployment request failed");

		// Assert
		assert_eq!(
			resp.status_code(),
			403,
			"Viewer must be denied with 403 for DeploymentUpdate"
		);
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_viewer_can_retrieve_deployment(#[future] db: DbFixture) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		let user = force_login_user(
			&client,
			&conn,
			&backend,
			"viewer_dep_get",
			"viewer-dep-get@example.com",
		)
		.await;
		let cluster_id = create_cluster(&client).await;
		let deployment_id = create_deployment(&client, cluster_id).await;
		set_membership_role(&conn, &user, MembershipRole::Viewer).await;

		// Act
		let resp = client
			.get(&format!("/api/deployments/{deployment_id}/"))
			.await
			.expect("Retrieve deployment request failed");

		// Assert
		assert_eq!(
			resp.status_code(),
			200,
			"Viewer must be allowed to read deployment"
		);
	}

	// =============================================================
	// Developer / Owner
	// =============================================================

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_developer_can_create_deployment(#[future] db: DbFixture) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		let user = force_login_user(
			&client,
			&conn,
			&backend,
			"dev_dep_create",
			"dev-dep-create@example.com",
		)
		.await;
		let cluster_id = create_cluster(&client).await;
		set_membership_role(&conn, &user, MembershipRole::Developer).await;

		// Act
		let data = json!({
			"app_name": "dev-app",
			"cluster_id": cluster_id,
			"image": "nginx:latest",
		});
		let resp = client
			.post("/api/deployments/", &data, "json")
			.await
			.expect("Create deployment request failed");

		// Assert
		assert_eq!(resp.status_code(), 201);
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_owner_can_delete_deployment(#[future] db: DbFixture) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		let _user = force_login_user(
			&client,
			&conn,
			&backend,
			"owner_dep_del",
			"owner-dep-del@example.com",
		)
		.await;
		let cluster_id = create_cluster(&client).await;
		let deployment_id = create_deployment(&client, cluster_id).await;

		// Act
		let resp = client
			.delete(&format!("/api/deployments/{deployment_id}/"))
			.await
			.expect("Delete deployment request failed");

		// Assert
		assert_eq!(resp.status_code(), 204);
	}
}
