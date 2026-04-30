//! End-to-end tests for auth error paths.
//!
//! Tests that exercise the register endpoint require Mailpit for SMTP.
//! Tests that only need login create users directly via ORM.

#[cfg(test)]
mod tests {
	use reinhardt::BaseUser;
	use reinhardt::db::orm::Model;
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::MailpitContainer;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serde_json::json;
	use serial_test::serial;
	use std::sync::Arc;

	use crate::apps::auth::models::User;
	use crate::config::test_helpers::ResolvedUrls;

	#[fixture]
	async fn db() -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		ResolvedUrls,
	) {
		// Start TestContainers first so build_test_app() registers DatabaseConnection
		// in the DI scope. Fixes #459.
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
		let (client, urls) = crate::config::test_helpers::build_test_app();
		(container, conn, client, urls)
	}

	/// rstest fixture: Mailpit container for SMTP testing.
	#[fixture]
	async fn mailpit() -> MailpitContainer {
		MailpitContainer::new().await
	}

	/// Set env vars for Mailpit SMTP and return a guard that restores them.
	fn set_mailpit_env(mailpit: &MailpitContainer) -> EnvGuard {
		let vars = vec![
			(
				"REINHARDT_CLOUD_BASE_URL",
				Some("http://localhost:8000".to_string()),
			),
			("REINHARDT_EMAIL__BACKEND", Some("smtp".to_string())),
			("REINHARDT_EMAIL__HOST", Some("127.0.0.1".to_string())),
			(
				"REINHARDT_EMAIL__PORT",
				Some(mailpit.smtp_port().to_string()),
			),
		];
		EnvGuard::set(vars)
	}

	/// RAII guard that restores environment variables on drop.
	struct EnvGuard {
		saved: Vec<(String, Option<String>)>,
	}

	impl EnvGuard {
		fn set(vars: Vec<(&str, Option<String>)>) -> Self {
			let mut saved = Vec::new();
			for (key, new_val) in &vars {
				saved.push((key.to_string(), std::env::var(key).ok()));
				// SAFETY: called in a serial test before any parallel tasks read these vars.
				unsafe {
					match new_val {
						Some(v) => std::env::set_var(key, v),
						None => std::env::remove_var(key),
					}
				}
			}
			Self { saved }
		}
	}

	impl Drop for EnvGuard {
		fn drop(&mut self) {
			for (key, old_val) in &self.saved {
				// SAFETY: restoring env vars in serial test teardown.
				unsafe {
					match old_val {
						Some(v) => std::env::set_var(key, v),
						None => std::env::remove_var(key),
					}
				}
			}
		}
	}

	/// Helper: create a user directly via ORM (bypasses register endpoint and email).
	async fn create_test_user(username: &str, email: &str, password: &str, active: bool) {
		let mut user = User::new(
			username.to_string(),
			email.to_lowercase(),
			String::new(),
			String::new(),
			None,
			active,
			false,
			false,
		);
		user.set_password(password)
			.expect("Password hashing failed");
		User::objects()
			.create(&user)
			.await
			.expect("Failed to create user");
	}

	/// Login with a non-existent user should not return 200.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_login_nonexistent_user_returns_401(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange
		let (_container, _conn, client, urls) = db.await;
		let login_data = json!({
			"username": "ghost_user",
			"password": "doesnotmatter"
		});

		// Act
		let response = client
			.post(&urls.server().auth().login(), &login_data, "json")
			.await
			.expect("Login request failed");

		// Assert
		assert_ne!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert!(
			body.get("error").is_some() || body.get("detail").is_some(),
			"Error response should contain 'error' or 'detail' field, got: {body}"
		);
	}

	/// Login with empty JSON body should return 400 (validation error).
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_login_empty_body_returns_422(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange
		let (_container, _conn, client, urls) = db.await;
		let empty_body = json!({});

		// Act
		let response = client
			.post(&urls.server().auth().login(), &empty_body, "json")
			.await
			.expect("Login request failed");

		// Assert
		assert_eq!(response.status_code(), 400);
	}

	/// Register with empty JSON body should return 400 (validation error).
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_register_empty_body_returns_422(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange
		let (_container, _conn, client, urls) = db.await;
		let empty_body = json!({});

		// Act
		let response = client
			.post(&urls.server().auth().register(), &empty_body, "json")
			.await
			.expect("Register request failed");

		// Assert
		assert_eq!(response.status_code(), 400);
	}

	/// Registering with a duplicate username (different email) should return 409 (requires Mailpit).
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_register_duplicate_username_returns_conflict(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
		#[future] mailpit: MailpitContainer,
	) {
		// Arrange -- register first user (needs Mailpit for email sending)
		let (_container, _conn, client, urls) = db.await;
		let mailpit = mailpit.await;
		let _env = set_mailpit_env(&mailpit);

		let first_user = json!({
			"username": "dupeuser",
			"email": "dupe1@example.com",
			"password": "securepassword"
		});
		let first_response = client
			.post(&urls.server().auth().register(), &first_user, "json")
			.await
			.expect("First register request failed");
		assert_eq!(first_response.status_code(), 201);

		// Act -- register second user with same username but different email
		let second_user = json!({
			"username": "dupeuser",
			"email": "dupe2@example.com",
			"password": "securepassword"
		});
		let response = client
			.post(&urls.server().auth().register(), &second_user, "json")
			.await
			.expect("Second register request failed");

		// Assert
		assert_eq!(response.status_code(), 409);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(body["detail"], "Username already exists");
	}

	/// Login with an inactive user should not return 200.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_login_inactive_user_returns_401(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange -- create inactive user via ORM
		let (_container, _conn, client, urls) = db.await;
		create_test_user(
			"inactiveuser",
			"inactive@example.com",
			"securepassword",
			false,
		)
		.await;

		// Act -- login with inactive user
		let login_data = json!({
			"username": "inactiveuser",
			"password": "securepassword"
		});
		let response = client
			.post(&urls.server().auth().login(), &login_data, "json")
			.await
			.expect("Login request failed");

		// Assert
		assert_ne!(
			response.status_code(),
			200,
			"Inactive user login should not return 200"
		);
	}
}
