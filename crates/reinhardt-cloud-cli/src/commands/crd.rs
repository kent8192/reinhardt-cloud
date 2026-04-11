//! CRD management commands for generating and inspecting custom resource definitions.

use std::path::PathBuf;

use clap::{Args, Subcommand};
use kube::CustomResourceExt;
use reinhardt_cloud_types::ReinhardtApp;

/// CRD management operations.
#[derive(Debug, Args)]
pub(crate) struct CrdArgs {
	#[command(subcommand)]
	pub(crate) command: CrdCommand,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CrdCommand {
	/// Generate CRD YAML to stdout or a file
	Generate(GenerateArgs),
}

/// Arguments for the `crd generate` subcommand.
#[derive(Debug, Args)]
pub(crate) struct GenerateArgs {
	/// Write output to a file instead of stdout
	#[arg(short, long)]
	pub(crate) output: Option<PathBuf>,
}

/// Execute the `crd` subcommand.
pub(crate) async fn execute(args: &CrdArgs) -> Result<(), Box<dyn std::error::Error>> {
	match &args.command {
		CrdCommand::Generate(gen_args) => generate(gen_args).await,
	}
}

/// Generate the `ReinhardtApp` CRD YAML from the Rust type definition.
async fn generate(args: &GenerateArgs) -> Result<(), Box<dyn std::error::Error>> {
	let crd = ReinhardtApp::crd();
	let yaml = serde_yaml::to_string(&crd)?;

	if let Some(path) = &args.output {
		// Ensure parent directory exists
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent)?;
		}
		std::fs::write(path, &yaml)?;
		eprintln!("CRD written to {}", path.display());
	} else {
		print!("{yaml}");
	}

	Ok(())
}
