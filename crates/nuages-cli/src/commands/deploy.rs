//! Deploy command: deploys an application to the nuages platform.

use clap::Args;

use crate::client::NuagesClient;

/// Deploy an application.
#[derive(Debug, Args)]
pub(crate) struct DeployArgs {
	/// Application name (falls back to `"app:latest"` when not specified)
	#[arg(short, long)]
	pub name: Option<String>,

	/// Docker image to deploy
	#[arg(short, long)]
	pub image: Option<String>,

	/// Number of replicas
	#[arg(short, long, default_value = "1")]
	pub replicas: u32,
}

/// Executes the deploy command.
pub(crate) async fn execute(
	args: &DeployArgs,
	_client: &NuagesClient,
) -> Result<(), Box<dyn std::error::Error>> {
	let app_name = args.name.as_deref().unwrap_or("default-app");
	let image = args.image.as_deref().unwrap_or("app:latest");
	println!(
		"Deploying {app_name} with image {image} ({} replicas)...",
		args.replicas
	);
	Err("deploy command is not yet implemented".into())
}
