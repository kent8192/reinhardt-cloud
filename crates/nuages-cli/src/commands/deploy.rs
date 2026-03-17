//! Deploy command: deploys an application to the nuages platform.

use clap::Args;
use std::path::PathBuf;

use crate::client::NuagesClient;
use nuages_types::nuages_toml::NuagesToml;

/// Deploy an application.
#[derive(Debug, Args)]
pub(crate) struct DeployArgs {
	/// Application name (overrides nuages.toml if set)
	#[arg(short, long)]
	pub name: Option<String>,

	/// Docker image to deploy (overrides nuages.toml if set)
	#[arg(short, long)]
	pub image: Option<String>,

	/// Number of replicas
	#[arg(short, long)]
	pub replicas: Option<u32>,

	/// Project directory (defaults to current directory)
	#[arg(short, long)]
	pub dir: Option<PathBuf>,
}

/// Reads nuages.toml from the project directory if it exists.
fn read_nuages_toml(dir: &std::path::Path) -> Option<NuagesToml> {
	let path = dir.join("nuages.toml");
	let content = std::fs::read_to_string(path).ok()?;
	toml::from_str(&content).ok()
}

/// Executes the deploy command.
pub(crate) async fn execute(
	args: &DeployArgs,
	_client: &NuagesClient,
) -> Result<(), Box<dyn std::error::Error>> {
	let project_dir = args.dir.clone().unwrap_or_else(|| PathBuf::from("."));

	// Try to read nuages.toml for zero-config deployment
	let (app_name, image, replicas) = if let Some(config) = read_nuages_toml(&project_dir) {
		let name = args.name.clone().unwrap_or(config.app.name.clone());
		let img = args.image.clone().unwrap_or(config.app.image.clone());
		let reps = args
			.replicas
			.unwrap_or(config.replicas.as_ref().map(|r| r.count as u32).unwrap_or(1));
		println!("Using configuration from nuages.toml");
		(name, img, reps)
	} else {
		let name = args
			.name
			.clone()
			.unwrap_or_else(|| "default-app".to_string());
		let img = args
			.image
			.clone()
			.unwrap_or_else(|| "app:latest".to_string());
		let reps = args.replicas.unwrap_or(1);
		(name, img, reps)
	};

	println!("Deploying {app_name} with image {image} ({replicas} replicas)...");

	// API call would go here (not yet implemented for this MVP)
	// let spec = config.to_reinhardt_app_spec();
	// client.create_app(&app_name, &spec).await?;

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_read_nuages_toml_exists() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("nuages.toml"),
			r#"
[app]
name = "test-app"
image = "test-app:v1"
"#,
		)
		.unwrap();

		// Act
		let config = read_nuages_toml(dir.path());

		// Assert
		let config = config.unwrap();
		assert_eq!(config.app.name, "test-app");
		assert_eq!(config.app.image, "test-app:v1");
	}

	#[rstest]
	fn test_read_nuages_toml_missing() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();

		// Act
		let config = read_nuages_toml(dir.path());

		// Assert
		assert!(config.is_none());
	}
}
