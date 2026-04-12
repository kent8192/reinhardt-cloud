//! End-to-end tests for clusters API endpoints.

#[cfg(test)]
mod tests {
	use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serde_json::json;
	use serial_test::serial;
	use std::sync::Arc;

	use crate::apps::auth::models::User;
	use crate::config::test_helpers::{TestUrls, test_app};

	#[fixture]
	async fn db(
		test_app: (APIClient, TestUrls),
	) -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		TestUrls,
	) {
		let (client, urls) = test_app;
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		(container, conn, client, urls)
	}

	/// Register a user, activate via ORM (bypassing email verification), then login.
	async fn register_and_get_session(client: &APIClient) -> String {
		let register_data = json!({
			"username": "testuser",
			"email": "test@example.com",
			"password": "securepassword123"
		});
		let resp = client
			.post("/api/auth/register/", &register_data, "json")
			.await
			.expect("Register request failed");
		assert_eq!(resp.status_code(), 201);

		// Activate user via ORM (bypassing email verification)
		let mut user = User::objects()
			.filter(
				User::field_username(),
				FilterOperator::Eq,
				FilterValue::String("testuser".to_string()),
			)
			.first()
			.await
			.expect("Failed to query user")
			.expect("User not found");
		user.is_active = true;
		User::objects()
			.update(&user)
			.await
			.expect("Failed to activate user");

		// Login to obtain session cookie
		let login_data = json!({
			"username": "testuser",
			"password": "securepassword123"
		});
		let login_resp = client
			.post("/api/auth/login/", &login_data, "json")
			.await
			.expect("Login request failed");
		assert_eq!(login_resp.status_code(), 200);
		let set_cookie = login_resp
			.header("Set-Cookie")
			.expect("Login response should have Set-Cookie header");
		let session_id = set_cookie
			.split(';')
			.next()
			.unwrap()
			.strip_prefix("sessionid=")
			.expect("Cookie should start with sessionid=");
		session_id.to_string()
	}

	/// Helper: set session cookie on client.
	async fn authenticate_client(client: &APIClient, session_id: &str) {
		client
			.set_header("Cookie", format!("sessionid={session_id}"))
			.await
			.expect("Failed to set Cookie header");
	}

	/// Verify unauthenticated GET /api/clusters/ returns 401.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_unauthenticated_clusters_returns_401(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			TestUrls,
		),
	) {
		// Arrange
		let (_container, _conn, client, _urls) = db.await;

		// Act
		let response = client
			.get("/api/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 401);
	}

	/// Verify GET /api/clusters/ returns empty list when authenticated.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_list_clusters_empty(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			TestUrls,
		),
	) {
		// Arrange
		let (_container, _conn, client, _urls) = db.await;
		let session = register_and_get_session(&client).await;
		authenticate_client(&client, &session).await;

		// Act
		let response = client
			.get("/api/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(body["items"], json!([]));
		assert_eq!(body["total"], 0);
		assert!(body["page"].is_number());
		assert!(body["page_size"].is_number());
	}

	/// Verify POST /api/clusters/ creates a cluster, then GET returns it.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_create_cluster_persists(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			TestUrls,
		),
	) {
		// Arrange
		let (_container, _conn, client, _urls) = db.await;
		let session = register_and_get_session(&client).await;
		authenticate_client(&client, &session).await;

		let cluster_data = json!({
			"name": "production-cluster",
			"api_url": "https://k8s.example.com:6443"
		});

		// Act -- create cluster
		let create_response = client
			.post("/api/clusters/", &cluster_data, "json")
			.await
			.expect("Create cluster request failed");

		// Assert -- creation response
		assert_eq!(create_response.status_code(), 201);
		let created: serde_json::Value = create_response
			.json()
			.expect("Failed to parse create response");
		assert_eq!(created["name"], "production-cluster");
		assert_eq!(created["api_url"], "https://k8s.example.com:6443");
		assert_eq!(created["is_active"], true);
		assert!(created["id"].is_number());

		// Act -- list clusters to verify persistence
		let list_response = client
			.get("/api/clusters/")
			.await
			.expect("List clusters request failed");

		// Assert -- cluster appears in list
		assert_eq!(list_response.status_code(), 200);
		let body: serde_json::Value = list_response.json().expect("Failed to parse list response");
		let items = body["items"].as_array().expect("items should be an array");
		assert_eq!(items.len(), 1);
		assert_eq!(items[0]["name"], "production-cluster");
		assert_eq!(items[0]["api_url"], "https://k8s.example.com:6443");
		assert_eq!(body["total"], 1);
		assert!(body["page"].is_number());
		assert!(body["page_size"].is_number());
	}
}
