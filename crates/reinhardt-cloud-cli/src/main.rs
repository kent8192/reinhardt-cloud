//! Reinhardt Cloud CLI tool for managing applications on the Reinhardt Cloud PaaS platform.

mod client;
mod commands;
mod config;
mod crd_version;
mod dockerfile_generator;
mod feature_detector;
mod settings_reader;
mod toml_generator;

use clap::{Parser, Subcommand};

use crate::client::ReinhardtCloudClient;
use crate::config::CliConfig;

/// Reinhardt Cloud PaaS command-line interface.
#[derive(Debug, Parser)]
#[command(
	name = "reinhardt-cloud",
	version,
	about = "Manage applications on the Reinhardt Cloud PaaS platform"
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
	/// Initialize reinhardt-cloud configuration for a reinhardt-web project
	Init(commands::init::InitArgs),
	/// Re-synchronize reinhardt-cloud.toml with current project state
	Sync(commands::sync::SyncArgs),
	/// Manage Git and registry credentials
	Credentials(commands::credentials::CredentialsArgs),
	/// Manage CRD manifests (generate, inspect)
	Crd(commands::crd::CrdArgs),
	/// Generate Terraform HCL from a ReinhardtApp infrastructure spec
	Terraform(commands::terraform::TerraformArgs),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
	// CLI --direct constructs kube rustls clients before invoking kubectl.
	// Install the provider explicitly so local E2E runs do not depend on
	// transitive rustls feature unification (Refs #638).
	rustls::crypto::ring::default_provider()
		.install_default()
		.ok();

	// Initialize OpenTelemetry tracing. The guard must live for the duration of
	// `main` so the OTLP span exporter is flushed on shutdown. When
	// `OTEL_EXPORTER_OTLP_ENDPOINT` is unset the path is effectively zero-cost.
	let _tracing_guard = reinhardt_cloud_telemetry::init_tracing(
		reinhardt_cloud_telemetry::TracingConfig::from_env("reinhardt-cloud-cli", false),
	)?;

	let cli = Cli::parse();

	// Load config from the platform config directory when present; fall back
	// to CliConfig::default() on missing-file or parse errors so `--help` and
	// first-run commands still function without any on-disk state.
	let config_file = config::config_path();
	let config = if config_file.exists() {
		match CliConfig::from_file(&config_file) {
			Ok(loaded) => loaded,
			Err(err) => {
				eprintln!(
					"warning: failed to load {}: {err}. Using defaults.",
					config_file.display()
				);
				CliConfig::default()
			}
		}
	} else {
		CliConfig::default()
	};

	let default_url = config.api_url();
	let base_url = cli.server.as_deref().unwrap_or(&default_url);
	let mut client = ReinhardtCloudClient::new(base_url)?;

	// Attach stored credentials when available. Absence is not fatal because
	// commands like `login` and `init` run before credentials exist. A
	// malformed or unreadable credentials file is also treated as non-fatal
	// (warning only) so first-run and CI scenarios are not blocked.
	match config::load_token() {
		Ok(Some(creds)) => {
			client = client.with_token(creds.token);
		}
		Ok(None) => {}
		Err(err) => {
			eprintln!(
				"warning: failed to load credentials: {err}. Continuing without authentication."
			);
		}
	}

	match &cli.command {
		Commands::Deploy(args) => commands::deploy::execute(args, &client).await,
		Commands::Status(args) => commands::status::execute(args, &client).await,
		Commands::Login(args) => commands::login::execute(args, &client).await,
		Commands::Init(args) => commands::init::execute(args).await,
		Commands::Sync(args) => commands::sync::execute(args).await,
		Commands::Credentials(args) => commands::credentials::execute(args).await,
		Commands::Crd(args) => commands::crd::execute(args).await,
		Commands::Terraform(args) => commands::terraform::execute(args).await,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_parse_deploy_command() {
		// Arrange
		let args = vec![
			"reinhardt-cloud",
			"deploy",
			"--name",
			"myapp",
			"--image",
			"myapp:v1",
		];

		// Act
		let cli = Cli::try_parse_from(args);

		// Assert
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_deploy_command_with_replicas() {
		// Arrange
		let args = vec![
			"reinhardt-cloud",
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
		let args = vec!["reinhardt-cloud", "status", "--name", "myapp"];

		// Act
		let cli = Cli::try_parse_from(args);

		// Assert
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_login_command() {
		// Arrange
		let args = vec!["reinhardt-cloud", "login", "--username", "alice"];

		// Act
		let cli = Cli::try_parse_from(args);

		// Assert
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_with_global_server_flag() {
		// Arrange
		let args = vec![
			"reinhardt-cloud",
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
		let args = vec!["reinhardt-cloud"];

		// Act
		let cli = Cli::try_parse_from(args);

		// Assert
		assert!(cli.is_err());
	}

	#[rstest]
	fn test_parse_init_command() {
		// Arrange
		let args = vec!["reinhardt-cloud", "init"];

		// Act
		let cli = Cli::try_parse_from(args);

		// Assert
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_init_command_with_dir() {
		// Arrange
		let args = vec!["reinhardt-cloud", "init", "--dir", "/some/path"];

		// Act
		let cli = Cli::try_parse_from(args);

		// Assert
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_sync_command() {
		// Arrange
		let args = vec!["reinhardt-cloud", "sync"];

		// Act
		let cli = Cli::try_parse_from(args);

		// Assert
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_sync_command_with_dir() {
		// Arrange
		let args = vec!["reinhardt-cloud", "sync", "--dir", "/some/path"];

		// Act
		let cli = Cli::try_parse_from(args);

		// Assert
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_credentials_set_command() {
		let args = vec![
			"reinhardt-cloud",
			"credentials",
			"set",
			"github",
			"--git-token",
			"ghp_xxx",
		];
		let cli = Cli::try_parse_from(args);
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_credentials_check_command() {
		let args = vec!["reinhardt-cloud", "credentials", "check", "my-app"];
		let cli = Cli::try_parse_from(args);
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_crd_generate_command() {
		let args = vec!["reinhardt-cloud", "crd", "generate"];
		let cli = Cli::try_parse_from(args);
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_crd_generate_with_output() {
		let args = vec![
			"reinhardt-cloud",
			"crd",
			"generate",
			"--output",
			"/tmp/crd.yaml",
		];
		let cli = Cli::try_parse_from(args);
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_init_command_with_force() {
		let args = vec!["reinhardt-cloud", "init", "--force"];
		let cli = Cli::try_parse_from(args);
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_sync_command_with_force() {
		let args = vec!["reinhardt-cloud", "sync", "--force"];
		let cli = Cli::try_parse_from(args);
		assert!(cli.is_ok());
	}

	#[rstest]
	fn test_parse_init_command_with_dir_and_force() {
		let args = vec!["reinhardt-cloud", "init", "--dir", "/some/path", "--force"];
		let cli = Cli::try_parse_from(args);
		assert!(cli.is_ok());
	}
}
