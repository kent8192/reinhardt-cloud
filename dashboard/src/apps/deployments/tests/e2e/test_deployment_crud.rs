//! End-to-end tests for deployments API endpoints.

#[cfg(test)]
mod tests {
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serde_json::json;
	use serial_test::serial;
	use std::sync::Arc;

	use crate::config::test_helpers::{TestUrls, test_app};

	#[fixture]
	async fn db(test_app: (APIClient, TestUrls)) -> (
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

	/// Helper: register a test user and return the session cookie value.
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
		let set_cookie = resp
			.header("Set-Cookie")
			.expect("Response should have Set-Cookie header");
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

	/// Verify unauthenticated GET /api/deployments/ returns 401.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_unauthenticated_deployments_returns_401(
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
			.get("/api/deployments/")
			.await
			.expect("List deployments request failed");

		// Assert
		assert_eq!(response.status_code(), 401);
	}

	/// Verify GET /api/deployments/ returns empty list when authenticated.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_list_deployments_empty(
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
			.get("/api/deployments/")
			.await
			.expect("List deployments request failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(body["items"], json!([]));
		assert_eq!(body["total"], 0);
		assert!(body["page"].is_number());
		assert!(body["page_size"].is_number());
	}
}
