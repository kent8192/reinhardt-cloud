//! End-to-end tests for the GitHub OAuth login flow.
//!
//! Drives `/api/auth/oauth/github/{start,callback}/` against a wiremock-rs
//! server impersonating GitHub. The dashboard's `GitHubProvider` is
//! pointed at the mock via the test-only env vars
//! `REINHARDT_CLOUD_OAUTH_GITHUB_{AUTHORIZE,TOKEN,USERINFO}_URL`, so the
//! provider's PKCE / state / token-exchange / userinfo paths actually
//! run — only the upstream HTTP destination is faked.
//!
//! Each scenario covers one branch of the linking decision tree from
//! issue #428: new-user creation (path d), email-verified match (path c),
//! and the email-collision rejection (`EmailConflict`) we ship as
//! defensive behavior when path (c) declines.

#[cfg(test)]
mod tests {
	use reinhardt::BaseUser;
	use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::APIClient;
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage};
	use rstest::*;
	use serde_json::json;
	use serial_test::serial;
	use std::sync::Arc;
	use wiremock::matchers::{method, path};
	use wiremock::{Mock, MockServer, ResponseTemplate};

	use crate::apps::auth::models::User;
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

	/// RAII guard that restores OAuth env vars on drop. Mirrors the helper
	/// used by other auth e2e tests so behavior under `serial` matches.
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

	/// Configure the dashboard to point at the mock GitHub. Returns the
	/// guard so env vars are restored when the test exits.
	fn configure_mock_github(mock: &MockServer) -> EnvGuard {
		let base = mock.uri();
		EnvGuard::set(vec![
			(
				"REINHARDT_CLOUD_OAUTH_GITHUB_CLIENT_ID",
				Some("test-client-id".to_string()),
			),
			(
				"REINHARDT_CLOUD_OAUTH_GITHUB_CLIENT_SECRET",
				Some("test-client-secret".to_string()),
			),
			(
				"REINHARDT_CLOUD_OAUTH_GITHUB_REDIRECT_URI",
				Some("http://localhost:8000/api/auth/oauth/github/callback/".to_string()),
			),
			(
				"REINHARDT_CLOUD_OAUTH_GITHUB_AUTHORIZE_URL",
				Some(format!("{base}/login/oauth/authorize")),
			),
			(
				"REINHARDT_CLOUD_OAUTH_GITHUB_TOKEN_URL",
				Some(format!("{base}/login/oauth/access_token")),
			),
			(
				"REINHARDT_CLOUD_OAUTH_GITHUB_USERINFO_URL",
				Some(format!("{base}/user")),
			),
		])
	}

	/// Mount the standard GitHub mock — token exchange returns a stable
	/// access token, userinfo returns the supplied JSON. Tests vary the
	/// userinfo body to drive different linking branches.
	async fn mount_github_mocks(mock: &MockServer, userinfo: serde_json::Value) {
		Mock::given(method("POST"))
			.and(path("/login/oauth/access_token"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({
				"access_token": "test-access-token",
				"token_type": "bearer",
				"scope": "user user:email"
			})))
			.mount(mock)
			.await;
		Mock::given(method("GET"))
			.and(path("/user"))
			.respond_with(ResponseTemplate::new(200).set_body_json(userinfo))
			.mount(mock)
			.await;
	}

	/// Run the full start → callback dance against the live router and
	/// return the callback `TestResponse` plus the `state` we extracted
	/// from `/start/`'s Location header.
	async fn drive_oauth_flow(
		client: &APIClient,
	) -> (reinhardt::test::response::TestResponse, String) {
		// /start/ → 302 to the (mocked) authorize URL with state= in the
		// query string. We don't actually follow the redirect; the
		// authorize URL is just a marker for state extraction.
		let start_resp = client
			.get("/api/auth/oauth/github/start/")
			.await
			.expect("start request");
		assert_eq!(start_resp.status_code(), 302, "start must 302");
		let location = start_resp
			.headers()
			.get("location")
			.expect("Location header")
			.to_str()
			.expect("Location utf8")
			.to_string();
		let parsed = url::Url::parse(&location).expect("authorize URL parses");
		let state = parsed
			.query_pairs()
			.find(|(k, _)| k == "state")
			.map(|(_, v)| v.into_owned())
			.expect("state param in authorize URL");

		// /callback/?code=&state= — the code we pass is a marker; the
		// mock token endpoint accepts any code and returns the same
		// access token. The state from begin_auth is URL-safe (b64url
		// charset) so no extra percent-encoding is required.
		let callback_url = format!("/api/auth/oauth/github/callback/?code=test-code&state={state}");
		let cb_resp = client.get(&callback_url).await.expect("callback request");
		(cb_resp, state)
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_new_user_full_oauth_flow_creates_user_and_session(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange — fresh DB, fresh GitHub mock, env pointed at it.
		let (_container, _conn, client, _urls) = db.await;
		let mock = MockServer::start().await;
		let _env = configure_mock_github(&mock);
		mount_github_mocks(
			&mock,
			json!({
				"id": 12345,
				"login": "octotest",
				"email": "octotest@example.com",
				"name": "Octo Test"
			}),
		)
		.await;

		// Act
		let (cb_resp, _state) = drive_oauth_flow(&client).await;

		// Assert — callback redirects to / with sessionid cookie set.
		assert_eq!(cb_resp.status_code(), 302, "callback must 302");
		let location = cb_resp
			.headers()
			.get("location")
			.expect("Location header")
			.to_str()
			.unwrap();
		assert_eq!(location, "/");
		let cookie = cb_resp
			.headers()
			.get("set-cookie")
			.expect("Set-Cookie header")
			.to_str()
			.unwrap();
		assert!(
			cookie.starts_with("sessionid="),
			"expected sessionid cookie, got: {cookie}"
		);

		// Assert — user persisted with no password (OAuth-only).
		let user = User::objects()
			.filter(
				User::field_email(),
				FilterOperator::Eq,
				FilterValue::String("octotest@example.com".to_string()),
			)
			.first()
			.await
			.unwrap()
			.expect("user created");
		assert!(user.password_hash.is_none());
		assert!(user.is_active());
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_email_verified_match_links_existing_user(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange — pre-existing local user that matches the OAuth email.
		// Provider asserts email_verified=true via the additional claim
		// so path (c) takes the merge branch.
		let (_c, _conn, client, _u) = db.await;
		let mut local = User::new(
			"existing_octo".to_string(),
			"matched@example.com".to_string(),
			String::new(),
			String::new(),
			None,
			true,
			false,
			false,
		);
		local.set_password("local-password-1234").unwrap();
		let local = User::objects().create(&local).await.unwrap();

		let mock = MockServer::start().await;
		let _env = configure_mock_github(&mock);
		mount_github_mocks(
			&mock,
			json!({
				"id": 9999,
				"login": "matchuser",
				"email": "matched@example.com",
				"email_verified": true,
				"name": "Matched User"
			}),
		)
		.await;

		// Act
		let (cb_resp, _state) = drive_oauth_flow(&client).await;

		// Assert — flow succeeded with redirect + cookie.
		assert_eq!(cb_resp.status_code(), 302);

		// Assert — no second user was created. The OAuth identity is
		// linked onto `local` (which still owns the email).
		let count = User::objects()
			.filter(
				User::field_email(),
				FilterOperator::Eq,
				FilterValue::String("matched@example.com".to_string()),
			)
			.all()
			.await
			.unwrap()
			.len();
		assert_eq!(count, 1, "no second user must be created on email-match");

		// Local password must remain usable — linking does not clear it.
		let after = User::objects()
			.filter(
				User::field_id(),
				FilterOperator::Eq,
				FilterValue::String(local.id.to_string()),
			)
			.first()
			.await
			.unwrap()
			.unwrap();
		assert!(after.password_hash.is_some());
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn test_email_unverified_collision_rejects_with_validation_error(
		#[future] db: (
			ContainerAsync<GenericImage>,
			Arc<DatabaseConnection>,
			APIClient,
			ResolvedUrls,
		),
	) {
		// Arrange — pre-existing user owns the email; provider does NOT
		// assert email_verified, so path (c) falls through to (d), and
		// the defensive duplicate-email check converts the would-be
		// UNIQUE crash into an EmailConflict (mapped to 422 in the
		// callback view).
		let (_c, _conn, client, _u) = db.await;
		let mut local = User::new(
			"original_owner".to_string(),
			"contested@example.com".to_string(),
			String::new(),
			String::new(),
			None,
			true,
			false,
			false,
		);
		local.set_password("password-1234").unwrap();
		User::objects().create(&local).await.unwrap();

		let mock = MockServer::start().await;
		let _env = configure_mock_github(&mock);
		mount_github_mocks(
			&mock,
			json!({
				"id": 7777,
				"login": "imposter",
				"email": "contested@example.com",
				"name": "Impostor"
			}),
		)
		.await;

		// Act
		let (cb_resp, _state) = drive_oauth_flow(&client).await;

		// Assert — request rejected, no second user created, no link
		// attached to the existing user.
		assert!(
			cb_resp.status_code() >= 400,
			"callback must fail on unverified collision, got {}",
			cb_resp.status_code()
		);
		let count = User::objects()
			.filter(
				User::field_email(),
				FilterOperator::Eq,
				FilterValue::String("contested@example.com".to_string()),
			)
			.all()
			.await
			.unwrap()
			.len();
		assert_eq!(count, 1, "no duplicate user must be created");
	}
}
