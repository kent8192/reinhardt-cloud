//! Cross-user isolation tests for clusters API.

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

	use reinhardt::ServerRouter;

	use crate::config::test_helpers::{force_login, force_login_user_with_org, session_backend};

	#[fixture]
	async fn db(
		session_backend: Arc<dyn AsyncSessionBackend>,
	) -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		Arc<ServerRouter>,
		Arc<dyn AsyncSessionBackend>,
	) {
		// Start the TestContainers database first so that build_test_app() can
		// register the DatabaseConnection in the DI singleton scope. This ensures
		// view handlers that inject Depends<DatabaseConnection> see the same DB
		// as helpers using create_with_conn. Fixes #459.
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		let (client, urls) = crate::config::test_helpers::build_test_app();
		(container, conn, client, urls, session_backend)
	}

	/// Helper: create a cluster for the current user's org and return its response body.
	async fn create_cluster(client: &APIClient, org_slug: &str, name: &str) -> serde_json::Value {
		let data = json!({
			"name": name,
			"api_url": "https://k8s.example.com:6443"
		});
		let resp = client
			.post(&format!("/api/orgs/{org_slug}/clusters/"), &data, "json")
			.await
			.expect("Create cluster request failed");
		assert_eq!(resp.status_code(), 201);
		resp.json().expect("Failed to parse create response")
	}

	/// UserA creates a cluster; UserB lists clusters and sees nothing.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_user_cannot_see_other_users_clusters(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<ServerRouter>,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;

		let (_user_a, org_a) =
			force_login_user_with_org(&client, &conn, &backend, "owner_a", "a@example.com").await;
		let slug_a = &org_a.slug;
		create_cluster(&client, slug_a, "cluster-a").await;

		let (_user_b, org_b) =
			force_login_user_with_org(&client, &conn, &backend, "owner_b", "b@example.com").await;
		let slug_b = &org_b.slug;

		// Act
		let response = client
			.get(&format!("/api/orgs/{slug_b}/clusters/"))
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(body["items"], json!([]));
		assert_eq!(body["total"], 0);
	}

	/// UserA creates 2 clusters, UserB creates 1 -- each sees only their own.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_user_sees_only_own_clusters(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<ServerRouter>,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;

		let (user_a, org_a) =
			force_login_user_with_org(&client, &conn, &backend, "owner_a", "a@example.com").await;
		let slug_a = org_a.slug.clone();
		create_cluster(&client, &slug_a, "cluster-a1").await;
		create_cluster(&client, &slug_a, "cluster-a2").await;

		let (_user_b, org_b) =
			force_login_user_with_org(&client, &conn, &backend, "owner_b", "b@example.com").await;
		let slug_b = &org_b.slug;
		create_cluster(&client, slug_b, "cluster-b1").await;

		// Act -- UserB lists clusters
		let resp_b = client
			.get(&format!("/api/orgs/{slug_b}/clusters/"))
			.await
			.expect("List clusters request failed");

		// Assert -- UserB sees only 1
		assert_eq!(resp_b.status_code(), 200);
		let body_b: serde_json::Value = resp_b.json().expect("Failed to parse JSON");
		assert_eq!(body_b["total"], 1);
		let items_b = body_b["items"]
			.as_array()
			.expect("items should be an array");
		assert_eq!(items_b.len(), 1);
		assert_eq!(items_b[0]["name"], "cluster-b1");

		// Act -- switch back to UserA
		force_login(&client, &backend, &user_a).await;
		let resp_a = client
			.get(&format!("/api/orgs/{slug_a}/clusters/"))
			.await
			.expect("List clusters request failed");

		// Assert -- UserA sees 2
		assert_eq!(resp_a.status_code(), 200);
		let body_a: serde_json::Value = resp_a.json().expect("Failed to parse JSON");
		assert_eq!(body_a["total"], 2);
		let items_a = body_a["items"]
			.as_array()
			.expect("items should be an array");
		assert_eq!(items_a.len(), 2);
	}

	/// Unauthenticated request (no session cookie) should return 401.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_no_session_cookie_returns_401(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<ServerRouter>,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange
		let (_container, _conn, client, _urls, _backend) = db.await;

		// Act -- use a placeholder slug; the auth middleware rejects before routing
		let response = client
			.get("/api/orgs/my-org/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 401);
	}

	/// An invalid session cookie value should return 401.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_invalid_session_cookie_returns_401(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			Arc<ServerRouter>,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange
		let (_container, _conn, client, _urls, _backend) = db.await;
		client
			.set_header(
				"Cookie",
				"sessionid=invalid-session-id-gibberish".to_string(),
			)
			.await
			.expect("Failed to set Cookie header");

		// Act
		let response = client
			.get("/api/orgs/my-org/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 401);
	}
}
