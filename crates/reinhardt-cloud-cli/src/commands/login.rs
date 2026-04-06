//! Login command: authenticates with the Reinhardt Cloud platform.

use clap::Args;
use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::client::ReinhardtCloudClient;
use crate::config::{Credentials, save_token};

/// Authenticate with the Reinhardt Cloud platform.
#[derive(Debug, Args)]
pub(crate) struct LoginArgs {
	/// Username
	#[arg(short, long)]
	pub username: String,
}

/// Request body for the login endpoint.
#[derive(Debug, Serialize)]
struct LoginRequest {
	username: String,
	password: String,
}

/// Response body from the login endpoint.
#[derive(Debug, Deserialize)]
struct LoginResponse {
	token: String,
}

/// Prompts for a password using hidden input.
///
/// Extracted as a function to allow tests to substitute input.
fn prompt_password() -> Result<String, Box<dyn std::error::Error>> {
	let password = rpassword::prompt_password("Password: ")
		.map_err(|e| format!("Failed to read password: {e}"))?;
	Ok(password)
}

/// Sends login credentials and returns the JWT token.
async fn send_login_request(
	client: &ReinhardtCloudClient,
	username: &str,
	password: &str,
) -> Result<String, Box<dyn std::error::Error>> {
	let body = LoginRequest {
		username: username.to_string(),
		password: password.to_string(),
	};

	let request = client.request(Method::POST, "/api/auth/login/")?;
	let response = request.json(&body).send().await?;

	if !response.status().is_success() {
		let status = response.status();
		let text = response
			.text()
			.await
			.unwrap_or_else(|_| "no response body".to_string());
		return Err(format!("Login failed (HTTP {status}): {text}").into());
	}

	let login_resp: LoginResponse = response.json().await.map_err(|e| {
		format!("Failed to parse login response (expected JSON with 'token' field): {e}")
	})?;

	Ok(login_resp.token)
}

/// Executes the login command.
pub(crate) async fn execute(
	args: &LoginArgs,
	client: &ReinhardtCloudClient,
) -> Result<(), Box<dyn std::error::Error>> {
	tracing::info!("attempting login for user: {}", args.username);

	let password = prompt_password()?;
	if password.is_empty() {
		return Err("Password cannot be empty".into());
	}

	println!("Logging in as {}...", args.username);

	let token = send_login_request(client, &args.username, &password).await?;

	let creds = Credentials {
		token,
		username: args.username.clone(),
	};
	save_token(&creds)?;

	println!(
		"Login successful. Credentials saved to {:?}",
		crate::config::credentials_path()
	);
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_login_request_serialization() {
		// Arrange
		let req = LoginRequest {
			username: "alice".to_string(),
			password: "secret123".to_string(),
		};

		// Act
		let json = serde_json::to_string(&req).unwrap();

		// Assert
		assert!(json.contains("\"username\":\"alice\""));
		assert!(json.contains("\"password\":\"secret123\""));
	}

	#[rstest]
	fn test_login_response_deserialization() {
		// Arrange
		let json = r#"{"token": "eyJhbGciOiJIUzI1NiJ9.test.sig"}"#;

		// Act
		let resp: LoginResponse = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(resp.token, "eyJhbGciOiJIUzI1NiJ9.test.sig");
	}

	#[rstest]
	fn test_login_response_missing_token_fails() {
		// Arrange
		let json = r#"{"error": "invalid credentials"}"#;

		// Act
		let result: Result<LoginResponse, _> = serde_json::from_str(json);

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_credentials_are_saved_and_readable() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let cred_path = dir.path().join("credentials.json");
		let creds = Credentials {
			token: "jwt-token-xyz".to_string(),
			username: "bob".to_string(),
		};

		// Act
		let json = serde_json::to_string_pretty(&creds).unwrap();
		std::fs::write(&cred_path, &json).unwrap();

		// Assert
		let content = std::fs::read_to_string(&cred_path).unwrap();
		let loaded: Credentials = serde_json::from_str(&content).unwrap();
		assert_eq!(loaded.token, "jwt-token-xyz");
		assert_eq!(loaded.username, "bob");
	}

	#[rstest]
	fn test_login_args_parses_username() {
		// Arrange & Act
		use clap::Parser;

		#[derive(Parser)]
		struct TestCli {
			#[command(flatten)]
			login: LoginArgs,
		}

		let cli = TestCli::try_parse_from(["test", "--username", "alice"]);

		// Assert
		assert!(cli.is_ok());
		assert_eq!(cli.unwrap().login.username, "alice");
	}
}
