//! Sync command: re-synchronizes `reinhardt-cloud.toml` with current project state.

use std::path::PathBuf;

use clap::Args;

use crate::dockerfile_generator::{self, SkipReason};
use crate::feature_detector::detect_project;
use crate::settings_reader::read_database_config;
use crate::toml_generator::{generate_config, generate_reinhardt_cloud_toml_string};

/// Re-synchronize `reinhardt-cloud.toml` with the current project state.
#[derive(Debug, Args)]
pub(crate) struct SyncArgs {
	/// Project directory (defaults to current directory)
	#[arg(short, long)]
	pub dir: Option<PathBuf>,

	/// Overwrite existing Dockerfile
	#[arg(long)]
	pub force: bool,
}

/// Executes the sync command.
pub(crate) async fn execute(args: &SyncArgs) -> Result<(), Box<dyn std::error::Error>> {
	let project_dir = args.dir.clone().unwrap_or_else(|| PathBuf::from("."));

	let reinhardt_cloud_toml_path = project_dir.join("reinhardt-cloud.toml");
	if !reinhardt_cloud_toml_path.exists() {
		return Err("reinhardt-cloud.toml not found. Run `reinhardt-cloud init` first.".into());
	}

	println!("Syncing reinhardt-cloud.toml with project state...");
	let metadata = detect_project(&project_dir)?;
	let db_config = read_database_config(&project_dir);
	let config = generate_config(&metadata, db_config.as_ref());
	let toml_string = generate_reinhardt_cloud_toml_string(&config);

	std::fs::write(&reinhardt_cloud_toml_path, &toml_string)?;
	println!("Updated reinhardt-cloud.toml");

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
			std::fs::write(&dockerfile_path, dockerfile.to_string())?;

			let pattern = if signals.pages { "pages" } else { "api" };
			let db_info = signals
				.database
				.as_deref()
				.map(|d| format!(" + {d}"))
				.unwrap_or_default();
			println!("Updated Dockerfile ({pattern}{db_info})");
		}
	}

	Ok(())
}
