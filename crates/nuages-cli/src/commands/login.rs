//! Login command: authenticates with the nuages platform.

use clap::Args;

use crate::client::NuagesClient;

/// Authenticate with the nuages platform.
#[derive(Debug, Args)]
pub(crate) struct LoginArgs {
	/// Username
	#[arg(short, long)]
	pub username: String,
}

/// Executes the login command.
pub(crate) async fn execute(
	args: &LoginArgs,
	_client: &NuagesClient,
) -> Result<(), Box<dyn std::error::Error>> {
	println!("Logging in as {}...", args.username);
	Err("login command is not yet implemented".into())
}
