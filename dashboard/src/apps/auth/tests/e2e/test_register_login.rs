//! End-to-end tests for auth API endpoints.
//!
//! These tests require both a PostgreSQL database and a Redis instance.
//! Tests that only need login create users directly via ORM (bypassing email).
//! Tests that exercise the register endpoint require Mailpit for SMTP.

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
	use crate::apps::organizations::models::Organization;
	use crate::config::test_helpers::{ResolvedUrls, test_app};

	#[fixture]
	async fn db(
		test_app: (APIClient, ResolvedUrls),
	) -> (
		ContainerAsync<GenericImage>,
		Arc<DatabaseConnection>,
		APIClient,
		ResolvedUrls,
	) {
		let (client, urls) = test_app;
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		let (container, conn) = postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations");
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

	/// Verify POST /api/auth/register/ creates an inactive user (requires Mailpit).
	///
	/// Asserts the new inactive-by-default contract: the response does not
	/// establish a session (no `sessionid` cookie) and the persisted user
	/// has `is_active = false` until the verification link is used.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_register_creates_inactive_user(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
		#[future] mailpit: MailpitContainer,
	) {
		// Arrange
		let (_container, _conn, client, urls) = db.await;
		let mailpit = mailpit.await;
		let _env = set_mailpit_env(&mailpit);

		let register_data = json!({
			"username": "newuser",
			"email": "newuser@example.com",
			"password": "securepassword"
		});

		// Act
		let response = client
			.post(&urls.server().auth().register(), &register_data, "json")
			.await
			.expect("Register request failed");

		// Assert — 201 with no session cookie
		assert_eq!(response.status_code(), 201);
		let set_cookie = response.header("Set-Cookie");
		assert!(
			set_cookie.is_none_or(|v| !v.contains("sessionid=")),
			"Registration must not establish a session; got Set-Cookie: {set_cookie:?}"
		);

		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(body["success"], true);
		assert!(body["user"].is_object());
		assert_eq!(body["user"]["username"], "newuser");

		// Assert — user persisted as inactive until email verification
		use reinhardt::db::orm::{FilterOperator, FilterValue};
		let user = User::objects()
			.filter(
				User::field_username(),
				FilterOperator::Eq,
				FilterValue::String("newuser".to_string()),
			)
			.first()
			.await
			.expect("Failed to query user")
			.expect("User should exist after successful registration");
		assert!(
			!user.is_active(),
			"Newly registered user must be inactive until email verification"
		);
	}

	/// Verify registration sets `Organization.created_by` to the new user's id.
	///
	/// This is the audit-trail invariant from #435: every Personal Org
	/// provisioned during registration must record the originating user as
	/// its creator, so an organization can always be traced back to the
	/// account that initially provisioned it.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_register_records_created_by_on_personal_org(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
		#[future] mailpit: MailpitContainer,
	) {
		// Arrange
		let (_container, _conn, client, urls) = db.await;
		let mailpit = mailpit.await;
		let _env = set_mailpit_env(&mailpit);

		let register_data = json!({
			"username": "auditor",
			"email": "auditor@example.com",
			"password": "securepassword"
		});

		// Act
		let response = client
			.post(&urls.server().auth().register(), &register_data, "json")
			.await
			.expect("Register request failed");
		assert_eq!(response.status_code(), 201);

		// Assert -- look up the new user, then look up the Personal Org by
		// `created_by = user.id` and verify exactly one match
		use reinhardt::db::orm::{FilterOperator, FilterValue};
		let user = User::objects()
			.filter(
				User::field_username(),
				FilterOperator::Eq,
				FilterValue::String("auditor".to_string()),
			)
			.first()
			.await
			.expect("Failed to query user")
			.expect("User should exist after successful registration");

		let org = Organization::objects()
			.filter(
				Organization::field_created_by(),
				FilterOperator::Eq,
				FilterValue::String(user.id.to_string()),
			)
			.first()
			.await
			.expect("Failed to query Organization by created_by")
			.expect("Personal Org should exist for the new user");

		assert_eq!(
			org.created_by, user.id,
			"Personal Org.created_by must equal the registering user's id (audit trail)",
		);
		assert_eq!(
			org.name, "auditor",
			"Personal Org name should default to the registering username",
		);
	}

	/// Verify POST /api/auth/login/ authenticates against DB and returns session cookie.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_login_returns_session_cookie(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange -- create active user via ORM
		let (_container, _conn, client, urls) = db.await;
		create_test_user("loginuser", "login@example.com", "testpassword", true).await;

		// Act -- login with same credentials
		let login_data = json!({
			"username": "loginuser",
			"password": "testpassword"
		});
		let response = client
			.post(&urls.server().auth().login(), &login_data, "json")
			.await
			.expect("Login request failed");

		// Assert
		assert_eq!(response.status_code(), 200);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(body["success"], true);
		assert!(body["user"].is_object());
	}

	/// Verify duplicate email registration returns 409 with email-specific message (requires Mailpit).
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_register_duplicate_email_returns_conflict(
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
			"username": "emailuser1",
			"email": "test@example.com",
			"password": "securepassword"
		});
		let first_response = client
			.post(&urls.server().auth().register(), &first_user, "json")
			.await
			.expect("First register request failed");
		assert_eq!(first_response.status_code(), 201);

		// Act -- register second user with same email but different username
		let second_user = json!({
			"username": "emailuser2",
			"email": "test@example.com",
			"password": "securepassword"
		});
		let response = client
			.post(&urls.server().auth().register(), &second_user, "json")
			.await
			.expect("Second register request failed");

		// Assert
		assert_eq!(response.status_code(), 409);
		let body: serde_json::Value = response.json().expect("Failed to parse JSON response");
		assert_eq!(body["error"], "Conflict");
		assert_eq!(body["detail"], "Email already exists");
	}

	/// Verify login with wrong password returns error.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_login_wrong_password_fails(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange -- create active user via ORM
		let (_container, _conn, client, urls) = db.await;
		create_test_user("failuser", "fail@example.com", "correctpassword", true).await;

		// Act -- login with wrong password
		let login_data = json!({
			"username": "failuser",
			"password": "wrongpassword"
		});
		let response = client
			.post(&urls.server().auth().login(), &login_data, "json")
			.await
			.expect("Login request failed");

		// Assert -- should not return 200
		assert_ne!(response.status_code(), 200);
	}

	/// Register with whitespace-padded username should be trimmed; login with trimmed name succeeds.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_register_whitespace_username_trimmed(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
		#[future] mailpit: MailpitContainer,
	) {
		// Arrange
		let (_container, _conn, client, urls) = db.await;
		let mailpit = mailpit.await;
		let _env = set_mailpit_env(&mailpit);

		let register_data = json!({
			"username": "  trimuser  ",
			"email": "trim@example.com",
			"password": "securepassword"
		});
		let reg_response = client
			.post(&urls.server().auth().register(), &register_data, "json")
			.await
			.expect("Register request failed");
		assert_eq!(reg_response.status_code(), 201);

		// Activate user via ORM (registration creates inactive user)
		use reinhardt::db::orm::{FilterOperator, FilterValue};
		let mut user = User::objects()
			.filter(
				User::field_username(),
				FilterOperator::Eq,
				FilterValue::String("trimuser".to_string()),
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

		// Act -- login with trimmed username
		let login_data = json!({
			"username": "trimuser",
			"password": "securepassword"
		});
		let response = client
			.post(&urls.server().auth().login(), &login_data, "json")
			.await
			.expect("Login request failed");

		// Assert
		assert_eq!(
			response.status_code(),
			200,
			"Login with trimmed username should succeed"
		);
	}
}
