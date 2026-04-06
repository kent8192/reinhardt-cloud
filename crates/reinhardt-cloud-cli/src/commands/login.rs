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
/// Prompts for a password on stderr (so it works in pipelines) and
/// authenticates via the dashboard API. On success, prints the JWT token.
pub(crate) async fn execute(
	args: &LoginArgs,
	client: &ReinhardtCloudClient,
) -> Result<(), Box<dyn std::error::Error>> {
	tracing::info!("attempting login for user: {}", args.username);

	let password = rpassword::prompt_password("Password: ")
		.map_err(|e| format!("failed to read password: {e}"))?;

	if password.is_empty() {
		return Err("password must not be empty".into());
	}

	match client.login(&args.username, &password).await {
		Ok(token) => {
			eprintln!("Login successful.");
			tracing::debug!("received JWT token ({} bytes)", token.len());
			// Print the token to stdout so callers can capture it
			// (e.g. `eval $(reinhardt-cloud login ...)`).
			// The success message goes to stderr to avoid polluting
			// the captured output.
			println!("{token}");
			Ok(())
		}
		Err(e) => Err(format!("login failed: {e}").into()),
	}
}
