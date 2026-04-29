//! End-to-end tests for deployments API endpoints.

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

	use crate::config::test_helpers::{ResolvedUrls, force_login_user, session_backend, test_app};

	#[fixture]
	async fn db(
		session_backend: Arc<dyn AsyncSessionBackend>,
	) -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		ResolvedUrls,
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

	/// Verify unauthenticated GET /api/deployments/ returns 401.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_unauthenticated_deployments_returns_401(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange
		let (_container, _conn, client, _urls, _backend) = db.await;

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
			ResolvedUrls,
			Arc<dyn AsyncSessionBackend>,
		),
	) {
		// Arrange
		let (_container, conn, client, _urls, backend) = db.await;
		force_login_user(&client, &conn, &backend, "testuser", "test@example.com").await;

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
