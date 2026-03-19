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
pub(crate) async fn execute(
	_args: &LoginArgs,
	_client: &ReinhardtCloudClient,
) -> Result<(), Box<dyn std::error::Error>> {
	tracing::info!("attempting login");
	Err("login command is not yet implemented".into())
}
