//! Login command: authenticates with the Reinhardt Cloud platform.

use clap::Args;

use crate::client::ReinhardtCloudClient;

/// Authenticate with the Reinhardt Cloud platform.
#[derive(Debug, Args)]
pub(crate) struct LoginArgs {
	/// Username
	#[arg(short, long)]
	pub username: String,
}

/// Executes the login command.
///
/// Dashboard login is handled by the Pages app through server functions.
/// The CLI no longer submits credentials to removed dashboard REST endpoints.
pub(crate) async fn execute(
	args: &LoginArgs,
	_client: &ReinhardtCloudClient,
) -> Result<(), Box<dyn std::error::Error>> {
	tracing::info!("attempting login for user: {}", args.username);

	Err(
		"dashboard login REST is no longer supported; use the dashboard Pages login form, \
		which calls the login server function directly."
			.into(),
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[tokio::test]
	async fn test_login_returns_unsupported_dashboard_rest_error() {
		// Arrange
		let args = LoginArgs {
			username: "alice".to_string(),
		};
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Act
		let result = execute(&args, &client).await;

		// Assert
		assert!(result.is_err());
		assert_eq!(
			result.unwrap_err().to_string(),
			"dashboard login REST is no longer supported; use the dashboard Pages login form, which calls the login server function directly."
		);
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
