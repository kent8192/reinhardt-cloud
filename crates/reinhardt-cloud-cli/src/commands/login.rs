//! Login command: authenticates with the Reinhardt Cloud platform.

use clap::Args;

use crate::client::{ClientError, ReinhardtCloudClient, UserInfo};

/// Authenticate with the Reinhardt Cloud platform using an API token.
#[derive(Debug, Args)]
pub(crate) struct LoginArgs {
	/// API token (skips the interactive prompt).
	/// Falls back to `REINHARDT_CLOUD_API_TOKEN`, then an interactive prompt.
	#[arg(short, long)]
	pub token: Option<String>,
}

/// Executes the login command: resolves the token, verifies it via `me()`,
/// and persists credentials locally so subsequent commands send authenticated
/// requests.
///
/// # Errors
///
/// Returns [`ClientError`] if the token is missing, invalid, expired,
/// revoked, or the control plane is unreachable.
pub(crate) async fn execute(
	args: &LoginArgs,
	client: &ReinhardtCloudClient,
) -> Result<UserInfo, ClientError> {
	let token = resolve_login_token(args);
	let authed = client.clone().with_token(token);
	let info = authed.me().await?;

	let creds = crate::config::Credentials {
		token: authed.token().unwrap_or_default().to_string(),
		username: info.username.clone(),
	};
	// Persist credentials. A write failure is non-fatal (e.g. a one-off
	// `--token` invocation) — log rather than abort a successful login.
	if let Err(e) = crate::config::save_token(&creds, &crate::config::credentials_path()) {
		tracing::warn!(
			"logged in as {} but failed to persist credentials: {e}",
			info.username
		);
	}
	tracing::info!("logged in as {}", info.username);
	Ok(info)
}

/// Resolve the API token from the flag, env var, or saved file, falling
/// back to an interactive prompt when none of those yield a token.
fn resolve_login_token(args: &LoginArgs) -> String {
	// Shared resolution (flag > env > file) used by every authenticated
	// command; login additionally falls back to an interactive prompt.
	if let Some(t) =
		crate::config::resolve_token(args.token.clone(), &crate::config::credentials_path())
		&& !t.is_empty()
	{
		return t;
	}
	// Interactive prompt (echo stays on; a TTY helper could hide it later).
	eprint!("Enter API token: ");
	let mut line = String::new();
	std::io::stdin()
		.read_line(&mut line)
		.expect("failed to read API token from stdin");
	line.trim().to_string()
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	use serial_test::serial;

	#[rstest]
	#[tokio::test]
	#[serial(env)]
	async fn test_login_validates_token_and_returns_username() {
		// Arrange — isolate credentials_path() under a temp HOME so the
		// best-effort save does not touch the real user config dir.
		// SAFETY: runs serially via #[serial(env)]; no other thread reads
		// HOME during this test.
		let dir = tempfile::tempdir().unwrap();
		let previous_home = std::env::var("HOME").ok();
		unsafe {
			std::env::set_var("HOME", dir.path());
		}

		let server = wiremock::MockServer::start().await;
		wiremock::Mock::given(wiremock::matchers::method("GET"))
			.and(wiremock::matchers::path("/api/auth/me/"))
			.respond_with(
				wiremock::ResponseTemplate::new(200)
					.set_body_json(serde_json::json!({ "id": "u-1", "username": "alice" })),
			)
			.mount(&server)
			.await;
		let client = ReinhardtCloudClient::new(&server.uri()).unwrap();
		let args = LoginArgs {
			token: Some("rct_valid".to_string()),
		};

		// Act
		let info = execute(&args, &client).await.unwrap();

		// Assert
		assert_eq!(info.username, "alice");

		// Cleanup
		// SAFETY: serial test; restore the prior HOME value.
		unsafe {
			match previous_home {
				Some(h) => std::env::set_var("HOME", h),
				None => std::env::remove_var("HOME"),
			}
		}
	}

	#[rstest]
	fn test_login_args_parses_token_flag() {
		// Arrange & Act
		use clap::Parser;
		#[derive(Parser)]
		struct TestCli {
			#[command(flatten)]
			login: LoginArgs,
		}
		let cli = TestCli::try_parse_from(["test", "--token", "rct_xyz"]);

		// Assert
		assert!(cli.is_ok());
		assert_eq!(cli.unwrap().login.token.as_deref(), Some("rct_xyz"));
	}
}
