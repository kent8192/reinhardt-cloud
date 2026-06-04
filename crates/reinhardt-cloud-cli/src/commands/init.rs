//! Init command: initializes `reinhardt-cloud.toml` for a reinhardt-web project.

use std::path::PathBuf;

use clap::Args;

use crate::dockerfile_generator::{self, SkipReason};
use crate::feature_detector::detect_project;
use crate::settings_reader::read_database_config;
use crate::toml_generator::{generate_config, generate_reinhardt_cloud_toml_string};

/// Initialize reinhardt-cloud configuration for the current project.
#[derive(Debug, Args)]
pub(crate) struct InitArgs {
	/// Project directory (defaults to current directory)
	#[arg(short, long)]
	pub dir: Option<PathBuf>,

	/// Overwrite existing files
	#[arg(long)]
	pub force: bool,
}

/// Executes the init command.
pub(crate) async fn execute(args: &InitArgs) -> Result<(), Box<dyn std::error::Error>> {
	let project_dir = args.dir.clone().unwrap_or_else(|| PathBuf::from("."));

	// Check if reinhardt-cloud.toml already exists
	let reinhardt_cloud_toml_path = project_dir.join("reinhardt-cloud.toml");
	if reinhardt_cloud_toml_path.exists() && !args.force {
		return Err(
			"reinhardt-cloud.toml already exists. Use --force to overwrite, or `reinhardt-cloud sync` to update.".into(),
		);
	}

	// Detect project
	println!("Detecting reinhardt-web project...");
	let metadata = detect_project(&project_dir)?;
	println!("  Found: {} v{}", metadata.name, metadata.version);

	// Read settings
	let db_config = read_database_config(&project_dir);
	if let Some(ref db) = db_config {
		println!("  Database: {} (from settings/base.toml)", db.engine);
	}

	// Print detected features
	if !metadata.features.is_empty() {
		println!("  Features: {}", metadata.features.join(", "));
	}

	// Generate and write reinhardt-cloud.toml
	let config = generate_config(&metadata, db_config.as_ref())?;
	let toml_string = generate_reinhardt_cloud_toml_string(&config);
	tokio::fs::write(&reinhardt_cloud_toml_path, &toml_string).await?;
	println!("Created reinhardt-cloud.toml");

	// Generate Dockerfile
	match dockerfile_generator::should_skip_dockerfile(&project_dir, &config, args.force) {
		SkipReason::CustomDockerfile => {
			println!("Skipped Dockerfile (custom path set in [source.build])");
		}
		SkipReason::AlreadyExists => {
			println!("Skipped Dockerfile (already exists — use --force to overwrite)");
		}
		SkipReason::None => {
			let signals = dockerfile_generator::collect_signals(&project_dir, &metadata, &config)?;
			let dockerfile = dockerfile_generator::generate(&signals);
			let dockerfile_path = project_dir.join("Dockerfile");
			tokio::fs::write(&dockerfile_path, dockerfile.to_string()).await?;

			let pattern = if signals.pages { "pages" } else { "api" };
			let db_info = signals
				.database
				.as_deref()
				.map(|d| format!(" + {d}"))
				.unwrap_or_default();
			println!("Created Dockerfile ({pattern}{db_info})");
		}
	}

	Ok(())
}
