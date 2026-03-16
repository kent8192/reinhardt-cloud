//! Nuages CLI tool for managing applications on the nuages PaaS platform.

// Scaffold: full API client and config usage will follow when commands are implemented
#[allow(dead_code)]
mod client;
mod commands;
#[allow(dead_code)]
mod config;

use clap::{Parser, Subcommand};

use crate::client::NuagesClient;
use crate::config::CliConfig;

/// Nuages PaaS command-line interface.
#[derive(Debug, Parser)]
#[command(
	name = "nuages",
	version,
	about = "Manage applications on the nuages PaaS platform"
)]
struct Cli {
	/// API server URL
	#[arg(long, global = true)]
	server: Option<String>,

	#[command(subcommand)]
	command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
	/// Deploy an application
	Deploy(commands::deploy::DeployArgs),
	/// Check deployment status
	Status(commands::status::StatusArgs),
	/// Authenticate with the platform
	Login(commands::login::LoginArgs),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	let cli = Cli::parse();
	let config = CliConfig::default();

	let default_url = config.api_url();
	let base_url = cli.server.as_deref().unwrap_or(&default_url);
	let client = NuagesClient::new(base_url);

	match &cli.command {
		Commands::Deploy(args) => commands::deploy::execute(args, &client).await,
		Commands::Status(args) => commands::status::execute(args, &client).await,
		Commands::Login(args) => commands::login::execute(args, &client).await,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_parse_deploy_command() {
		// Arrange
		let args = vec!["nuages", "deploy", "--name", "myapp", "--image", "myapp:v1"];

		// Act
		let cli = Cli::try_parse_from(args);

		// Assert
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_deploy_command_with_replicas() {
		// Arrange
		let args = vec![
			"nuages",
			"deploy",
			"--name",
			"myapp",
			"--image",
			"myapp:v1",
			"--replicas",
			"3",
		];

		// Act
		let cli = Cli::try_parse_from(args);

		// Assert
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_status_command() {
		// Arrange
		let args = vec!["nuages", "status", "--name", "myapp"];

		// Act
		let cli = Cli::try_parse_from(args);

		// Assert
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_login_command() {
		// Arrange
		let args = vec!["nuages", "login", "--username", "alice"];

		// Act
		let cli = Cli::try_parse_from(args);

		// Assert
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_with_global_server_flag() {
		// Arrange
		let args = vec![
			"nuages",
			"--server",
			"http://custom:9000",
			"status",
			"--name",
			"myapp",
		];

		// Act
		let cli = Cli::try_parse_from(args);

		// Assert
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_missing_subcommand_fails() {
		// Arrange
		let args = vec!["nuages"];

		// Act
		let cli = Cli::try_parse_from(args);

		// Assert
		assert!(cli.is_err());
	}
}
