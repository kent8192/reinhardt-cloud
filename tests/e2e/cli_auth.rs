//! End-to-end tests for CLI control-plane authentication (#715).
//!
//! Drives the real dashboard `/api/auth/me/` endpoint with a bearer token to
//! verify the `ApiTokenAuthMiddleware` + `api_me` integration end to end.
//!
//! Gated behind `REINHARDT_CLOUD_CLI_AUTH_E2E=1` (mirrors the
//! `source_pipeline` RUN_ENV pattern) so the suite is skipped by default.
//! Requires a running dashboard (Phase 1, #720) at
//! `$REINHARDT_CLOUD_E2E_BASE_URL` and a token issued via
//! `manage create-api-token` in `$REINHARDT_CLOUD_E2E_TOKEN`. Full harness
//! provisioning (auto-start the dashboard + issue / revoke tokens) lands
//! once #720 and #722 merge; until then the scenarios below exercise the
//! contract against an externally-provisioned dashboard.

use reqwest::StatusCode;

const RUN_ENV: &str = "REINHARDT_CLOUD_CLI_AUTH_E2E";

fn enabled() -> bool {
	std::env::var(RUN_ENV).ok().as_deref() == Some("1")
}

fn base_url() -> String {
	std::env::var("REINHARDT_CLOUD_E2E_BASE_URL")
		.unwrap_or_else(|_| "http://localhost:8000".to_string())
}

fn valid_token() -> Option<String> {
	std::env::var("REINHARDT_CLOUD_E2E_TOKEN").ok()
}

/// A valid bearer token authenticates and returns the user.
#[tokio::test]
async fn api_me_accepts_valid_bearer_token() {
	// Arrange
	if !enabled() {
		eprintln!("skipping CLI auth E2E; set {RUN_ENV}=1 to run it");
		return;
	}
	let token = valid_token().expect("REINHARDT_CLOUD_E2E_TOKEN must be set");
	let client = reqwest::Client::new();

	// Act
	let resp = client
		.get(format!("{}/api/auth/me/", base_url()))
		.bearer_auth(&token)
		.send()
		.await
		.expect("request to control plane");

	// Assert
	assert_eq!(resp.status(), StatusCode::OK);
	let body: serde_json::Value = resp.json().await.expect("decode response body");
	let username = body["username"]
		.as_str()
		.expect("response carries a username field");
	assert!(!username.is_empty());
}

/// A missing token is rejected with 401.
#[tokio::test]
async fn api_me_rejects_missing_token() {
	// Arrange
	if !enabled() {
		eprintln!("skipping CLI auth E2E; set {RUN_ENV}=1 to run it");
		return;
	}
	let client = reqwest::Client::new();

	// Act
	let resp = client
		.get(format!("{}/api/auth/me/", base_url()))
		.send()
		.await
		.expect("request to control plane");

	// Assert
	assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// An unknown / malformed bearer token is rejected with 401.
#[tokio::test]
async fn api_me_rejects_unknown_token() {
	// Arrange
	if !enabled() {
		eprintln!("skipping CLI auth E2E; set {RUN_ENV}=1 to run it");
		return;
	}
	// A deliberately-invalid token (assembled so no literal secret appears).
	let bogus_token = format!("rct_{}", "unknown-placeholder-not-real");
	let client = reqwest::Client::new();

	// Act
	let resp = client
		.get(format!("{}/api/auth/me/", base_url()))
		.bearer_auth(&bogus_token)
		.send()
		.await
		.expect("request to control plane");

	// Assert
	assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// A revoked token is rejected with 401.
///
/// Requires `$REINHARDT_CLOUD_E2E_REVOKED_TOKEN` — a token that was issued
/// then revoked via `manage revoke-api-token` by the harness.
#[tokio::test]
async fn api_me_rejects_revoked_token() {
	// Arrange
	if !enabled() {
		eprintln!("skipping CLI auth E2E; set {RUN_ENV}=1 to run it");
		return;
	}
	let Some(token) = std::env::var("REINHARDT_CLOUD_E2E_REVOKED_TOKEN").ok() else {
		eprintln!(
			"skipping revoked-token scenario; set REINHARDT_CLOUD_E2E_REVOKED_TOKEN to run it"
		);
		return;
	};
	let client = reqwest::Client::new();

	// Act
	let resp = client
		.get(format!("{}/api/auth/me/", base_url()))
		.bearer_auth(&token)
		.send()
		.await
		.expect("request to control plane");

	// Assert
	assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
