//! End-to-end tests for email verification flow.
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

	use crate::config::test_helpers::{ResolvedUrls, test_app};

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

	/// rstest fixture: database + app client.
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

	/// Helper: fetch messages from Mailpit HTTP API.
	async fn fetch_messages(mailpit: &MailpitContainer) -> Vec<MailpitMessageSummary> {
		let url = format!("{}/api/v1/messages", mailpit.http_url());
		let resp = reqwest::get(&url)
			.await
			.expect("Failed to fetch Mailpit messages");
		let data: MessagesResponse = resp.json().await.expect("Failed to parse Mailpit response");
		data.messages
	}

	/// Helper: fetch a single message body.
	async fn fetch_message_text(mailpit: &MailpitContainer, id: &str) -> String {
		let url = format!("{}/api/v1/message/{}", mailpit.http_url(), id);
		let resp = reqwest::get(&url).await.expect("Failed to fetch message");
		let msg: MailpitMessage = resp.json().await.expect("Failed to parse message");
		msg.text
	}

	/// Helper: delete all messages.
	async fn delete_all_messages(mailpit: &MailpitContainer) {
		let url = format!("{}/api/v1/messages", mailpit.http_url());
		reqwest::Client::new().delete(&url).send().await.ok();
	}

	/// Poll Mailpit until at least `expected` messages arrive, with timeout.
	async fn poll_messages(
		mailpit: &MailpitContainer,
		expected: usize,
		timeout: Duration,
	) -> Vec<MailpitMessageSummary> {
		let deadline = tokio::time::Instant::now() + timeout;
		loop {
			let messages = fetch_messages(mailpit).await;
			if messages.len() >= expected {
				return messages;
			}
			if tokio::time::Instant::now() >= deadline {
				panic!(
					"Timed out waiting for {expected} Mailpit message(s) (got {})",
					messages.len()
				);
			}
			tokio::time::sleep(Duration::from_millis(100)).await;
		}
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

	/// Helper: extract token from verification email body.
	///
	/// Looks for a URL pattern like `/api/auth/verify-email/{token}/`
	fn extract_verify_token(text: &str) -> Option<String> {
		let marker = "/api/auth/verify-email/";
		let start = text.find(marker)? + marker.len();
		let rest = &text[start..];
		let end = rest.find('/')?;
		Some(rest[..end].to_string())
	}

	/// Register creates an inactive user and sends a verification email.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_register_sends_verification_email(
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
		delete_all_messages(&mailpit).await;

		let _env = set_mailpit_env(&mailpit);

		let register_data = json!({
			"username": "verifyuser",
			"email": "verify@example.com",
			"password": "securepassword"
		});

		// Act
		let response = client
			.post(&urls.server().auth().register(), &register_data, "json")
			.await
			.expect("Register request failed");

		// Assert — registration returns 201
		assert_eq!(response.status_code(), 201);
		let body: serde_json::Value = response.json().expect("Failed to parse response");
		assert_eq!(body["success"], true);

		// Poll Mailpit until the verification email arrives
		let messages = poll_messages(&mailpit, 1, Duration::from_secs(5)).await;
		assert_eq!(messages.len(), 1, "Expected exactly one verification email");

		// Extract token from email body
		let text = fetch_message_text(&mailpit, &messages[0].id).await;
		let token = extract_verify_token(&text);
		assert!(
			token.is_some(),
			"Verification token not found in email body"
		);
	}

	/// Verify-email endpoint activates an inactive user.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_verify_email_activates_user(
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
		delete_all_messages(&mailpit).await;

		let _env = set_mailpit_env(&mailpit);

		let register_data = json!({
			"username": "activateuser",
			"email": "activate@example.com",
			"password": "securepassword"
		});
		client
			.post(&urls.server().auth().register(), &register_data, "json")
			.await
			.expect("Register request failed");

		let messages = poll_messages(&mailpit, 1, Duration::from_secs(5)).await;
		let text = fetch_message_text(&mailpit, &messages[0].id).await;
		let token = extract_verify_token(&text).expect("Token not found");

		// Act — verify email
		let verify_url = urls.server().auth().verify_email(&token);
		let verify_response = client
			.get(&verify_url)
			.await
			.expect("Verify request failed");

		// Assert
		assert_eq!(verify_response.status_code(), 200);
		let body: serde_json::Value = verify_response.json().expect("Failed to parse response");
		assert_eq!(body["success"], true);

		// Login should now succeed
		let login_data = json!({
			"username": "activateuser",
			"password": "securepassword"
		});
		let login_response = client
			.post(&urls.server().auth().login(), &login_data, "json")
			.await
			.expect("Login request failed");
		assert_eq!(login_response.status_code(), 200);
	}

	/// Unverified user cannot login.
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_unverified_user_cannot_login(
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
			"username": "noverify",
			"email": "noverify@example.com",
			"password": "securepassword"
		});
		client
			.post(&urls.server().auth().register(), &register_data, "json")
			.await
			.expect("Register request failed");

		// Act — try login without verifying
		let login_data = json!({
			"username": "noverify",
			"password": "securepassword"
		});
		let response = client
			.post(&urls.server().auth().login(), &login_data, "json")
			.await
			.expect("Login request failed");

		// Assert — login should fail (user is inactive)
		assert_ne!(response.status_code(), 200);
	}

	/// Double verification is idempotent (returns 200 both times).
	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_double_verification_is_idempotent(
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
		delete_all_messages(&mailpit).await;

		let _env = set_mailpit_env(&mailpit);

		let register_data = json!({
			"username": "doubleuser",
			"email": "double@example.com",
			"password": "securepassword"
		});
		client
			.post(&urls.server().auth().register(), &register_data, "json")
			.await
			.expect("Register failed");

		let messages = poll_messages(&mailpit, 1, Duration::from_secs(5)).await;
		let text = fetch_message_text(&mailpit, &messages[0].id).await;
		let token = extract_verify_token(&text).expect("Token not found");
		let verify_url = urls.server().auth().verify_email(&token);

		// Act — verify twice
		let first = client.get(&verify_url).await.expect("First verify failed");
		let second = client.get(&verify_url).await.expect("Second verify failed");

		// Assert — both succeed
		assert_eq!(first.status_code(), 200);
		assert_eq!(second.status_code(), 200);
	}
}
