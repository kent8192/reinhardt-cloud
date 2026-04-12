//! End-to-end tests for password forgot/reset flow.
//!
//! These tests require PostgreSQL (via TestContainers), Redis, and
//! Mailpit (via TestContainers) for SMTP email delivery verification.

#[cfg(test)]
mod tests {
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::MailpitContainer;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serde_json::json;
	use serial_test::serial;
	use std::sync::Arc;
	use std::time::Duration;

	use crate::config::test_helpers::{TestUrls, test_app};

	/// Mailpit API message summary.
	#[derive(Debug, serde::Deserialize)]
	struct MailpitMessageSummary {
		#[serde(rename = "ID")]
		id: String,
	}

	/// Mailpit API full message.
	#[derive(Debug, serde::Deserialize)]
	struct MailpitMessage {
		#[serde(rename = "Text")]
		text: String,
	}

	#[derive(Debug, serde::Deserialize)]
	#[allow(dead_code)]
	struct MessagesResponse {
		total: usize,
		messages_count: usize,
		start: usize,
		messages: Vec<MailpitMessageSummary>,
	}

	/// rstest fixture: Mailpit container for SMTP testing.
	#[fixture]
	async fn mailpit() -> MailpitContainer {
		MailpitContainer::new().await
	}

	/// rstest fixture: database + app client + email verification for a pre-registered user.
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

	async fn fetch_messages(mailpit: &MailpitContainer) -> Vec<MailpitMessageSummary> {
		let url = format!("{}/api/v1/messages", mailpit.http_url());
		let resp = reqwest::get(&url).await.expect("Mailpit fetch failed");
		let data: MessagesResponse = resp.json().await.expect("Mailpit parse failed");
		data.messages
	}

	async fn fetch_message_text(mailpit: &MailpitContainer, id: &str) -> String {
		let url = format!("{}/api/v1/message/{}", mailpit.http_url(), id);
		let resp = reqwest::get(&url).await.expect("Mailpit fetch failed");
		let msg: MailpitMessage = resp.json().await.expect("Mailpit parse failed");
		msg.text
	}

	async fn delete_all_messages(mailpit: &MailpitContainer) {
		let url = format!("{}/api/v1/messages", mailpit.http_url());
		reqwest::Client::new().delete(&url).send().await.ok();
	}

	/// Extract reset token from password reset email body.
	fn extract_reset_token(text: &str) -> Option<String> {
		let marker = "/api/auth/reset-password/";
		let start = text.find(marker)? + marker.len();
		let rest = &text[start..];
		let end = rest.find('/')?;
		Some(rest[..end].to_string())
	}

	/// Helper: register and verify a user (active user ready for testing).
	async fn register_and_verify_user(
		client: &APIClient,
		urls: &TestUrls,
		mailpit: &MailpitContainer,
		username: &str,
		email: &str,
		password: &str,
	) {
		delete_all_messages(mailpit).await;

		// SAFETY: Called in a serial test before any parallel tasks read this var.
		unsafe {
			std::env::set_var("REINHARDT_CLOUD_BASE_URL", "http://localhost:8000");
			std::env::set_var("REINHARDT_EMAIL__BACKEND", "smtp");
			std::env::set_var("REINHARDT_EMAIL__HOST", "127.0.0.1");
			std::env::set_var("REINHARDT_EMAIL__PORT", mailpit.smtp_port().to_string());
		}

		let data = json!({
			"username": username,
			"email": email,
			"password": password
		});
		client
			.post(&urls.auth_register, &data, "json")
			.await
			.expect("Register failed");

		tokio::time::sleep(Duration::from_millis(500)).await;
		let messages = fetch_messages(mailpit).await;
		let text = fetch_message_text(mailpit, &messages[0].id).await;

		let marker = "/api/auth/verify-email/";
		let start = text.find(marker).expect("No verify URL") + marker.len();
		let rest = &text[start..];
		let end = rest.find('/').expect("No trailing slash");
		let token = &rest[..end];

		let verify_url = urls.auth_verify_email(token);
		client.get(&verify_url).await.expect("Verify failed");
	}

	/// Forgot-password sends email for existing active user.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_forgot_password_sends_email(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			TestUrls,
		),
		#[future] mailpit: MailpitContainer,
	) {
		// Arrange
		let (_container, _conn, client, urls) = db.await;
		let mailpit = mailpit.await;

		register_and_verify_user(
			&client,
			&urls,
			&mailpit,
			"resetuser",
			"reset@example.com",
			"oldpassword",
		)
		.await;
		delete_all_messages(&mailpit).await;

		// Act
		let forgot_data = json!({ "email": "reset@example.com" });
		let response = client
			.post(&urls.auth_forgot_password, &forgot_data, "json")
			.await
			.expect("Forgot-password request failed");

		// Assert
		assert_eq!(response.status_code(), 200);

		tokio::time::sleep(Duration::from_millis(500)).await;
		let messages = fetch_messages(&mailpit).await;
		assert_eq!(messages.len(), 1, "Expected one reset email");

		let text = fetch_message_text(&mailpit, &messages[0].id).await;
		assert!(
			extract_reset_token(&text).is_some(),
			"Reset token not found in email"
		);
	}

	/// Forgot-password returns 200 for non-existent email (no enumeration).
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_forgot_password_nonexistent_email_returns_200(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			TestUrls,
		),
	) {
		// Arrange
		let (_container, _conn, client, urls) = db.await;

		// Act
		let forgot_data = json!({ "email": "noone@example.com" });
		let response = client
			.post(&urls.auth_forgot_password, &forgot_data, "json")
			.await
			.expect("Forgot-password request failed");

		// Assert — always 200 to prevent enumeration
		assert_eq!(response.status_code(), 200);
	}

	/// Reset-password with valid token changes the password.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_reset_password_changes_password(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			TestUrls,
		),
		#[future] mailpit: MailpitContainer,
	) {
		// Arrange
		let (_container, _conn, client, urls) = db.await;
		let mailpit = mailpit.await;

		register_and_verify_user(
			&client,
			&urls,
			&mailpit,
			"resetpwuser",
			"resetpw@example.com",
			"oldpassword",
		)
		.await;
		delete_all_messages(&mailpit).await;

		// Request reset email
		let forgot_data = json!({ "email": "resetpw@example.com" });
		client
			.post(&urls.auth_forgot_password, &forgot_data, "json")
			.await
			.expect("Forgot-password failed");

		tokio::time::sleep(Duration::from_millis(500)).await;
		let messages = fetch_messages(&mailpit).await;
		let text = fetch_message_text(&mailpit, &messages[0].id).await;
		let token = extract_reset_token(&text).expect("Token not found");

		// Act — reset password
		let reset_url = urls.auth_reset_password(&token);
		let reset_data = json!({ "new_password": "newpassword123" });
		let response = client
			.post(&reset_url, &reset_data, "json")
			.await
			.expect("Reset-password failed");

		// Assert
		assert_eq!(response.status_code(), 200);

		// Old password should no longer work
		let login_old = json!({
			"username": "resetpwuser",
			"password": "oldpassword"
		});
		let old_resp = client
			.post(&urls.auth_login, &login_old, "json")
			.await
			.expect("Login failed");
		assert_ne!(old_resp.status_code(), 200);

		// New password should work
		let login_new = json!({
			"username": "resetpwuser",
			"password": "newpassword123"
		});
		let new_resp = client
			.post(&urls.auth_login, &login_new, "json")
			.await
			.expect("Login failed");
		assert_eq!(new_resp.status_code(), 200);
	}

	/// Used reset token cannot be reused (password_hash changed).
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_used_reset_token_is_invalid(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			TestUrls,
		),
		#[future] mailpit: MailpitContainer,
	) {
		// Arrange
		let (_container, _conn, client, urls) = db.await;
		let mailpit = mailpit.await;

		register_and_verify_user(
			&client,
			&urls,
			&mailpit,
			"reuseuser",
			"reuse@example.com",
			"oldpassword",
		)
		.await;
		delete_all_messages(&mailpit).await;

		let forgot_data = json!({ "email": "reuse@example.com" });
		client
			.post(&urls.auth_forgot_password, &forgot_data, "json")
			.await
			.expect("Forgot-password failed");

		tokio::time::sleep(Duration::from_millis(500)).await;
		let messages = fetch_messages(&mailpit).await;
		let text = fetch_message_text(&mailpit, &messages[0].id).await;
		let token = extract_reset_token(&text).expect("Token not found");

		// Use the token once
		let reset_url = urls.auth_reset_password(&token);
		let reset_data = json!({ "new_password": "newpassword123" });
		let first = client
			.post(&reset_url, &reset_data, "json")
			.await
			.expect("First reset failed");
		assert_eq!(first.status_code(), 200);

		// Act — try to use the same token again
		let second_data = json!({ "new_password": "anotherpassword" });
		let second = client
			.post(&reset_url, &second_data, "json")
			.await
			.expect("Second reset failed");

		// Assert — should fail because password_hash changed
		assert_ne!(second.status_code(), 200);
	}
}
