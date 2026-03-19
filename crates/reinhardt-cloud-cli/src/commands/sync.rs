//! Sync command: re-synchronizes `nuages.toml` with current project state.

use std::path::PathBuf;

use clap::Args;

use crate::feature_detector::detect_project;
use crate::settings_reader::read_database_config;
use crate::toml_generator::{generate_config, generate_nuages_toml_string};

/// Re-synchronize `nuages.toml` with the current project state.
#[derive(Debug, Args)]
pub(crate) struct SyncArgs {
	/// Project directory (defaults to current directory)
	#[arg(short, long)]
	pub dir: Option<PathBuf>,
}

/// Executes the sync command.
pub(crate) async fn execute(args: &SyncArgs) -> Result<(), Box<dyn std::error::Error>> {
	let project_dir = args.dir.clone().unwrap_or_else(|| PathBuf::from("."));

	let nuages_toml_path = project_dir.join("nuages.toml");
	if !nuages_toml_path.exists() {
		return Err("nuages.toml not found. Run `nuages init` first.".into());
	}

	println!("Syncing nuages.toml with project state...");
	let metadata = detect_project(&project_dir)?;
	let db_config = read_database_config(&project_dir);
	let config = generate_config(&metadata, db_config.as_ref());
	let toml_string = generate_nuages_toml_string(&config);

	std::fs::write(&nuages_toml_path, &toml_string)?;
	println!("Updated nuages.toml");

	Ok(())
}
