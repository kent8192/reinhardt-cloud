//! Status command: checks the deployment status.

use clap::Args;

use crate::client::ReinhardtCloudClient;

/// Check deployment status.
#[derive(Debug, Args)]
pub(crate) struct StatusArgs {
	/// Application name to check
	#[arg(short, long)]
	pub name: Option<String>,
}

/// Executes the status command.
pub(crate) async fn execute(
	args: &StatusArgs,
	_client: &ReinhardtCloudClient,
) -> Result<(), Box<dyn std::error::Error>> {
	let app_name = args.name.as_deref().unwrap_or("default-app");
	println!("Checking status of {app_name}...");
	Err("status command is not yet implemented".into())
}
