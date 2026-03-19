//! Init command: initializes `nuages.toml` for a reinhardt-web project.

use std::path::PathBuf;

use clap::Args;

use crate::feature_detector::detect_project;
use crate::settings_reader::read_database_config;
use crate::toml_generator::{generate_config, generate_nuages_toml_string};

/// Initialize nuages configuration for the current project.
#[derive(Debug, Args)]
pub(crate) struct InitArgs {
	/// Project directory (defaults to current directory)
	#[arg(short, long)]
	pub dir: Option<PathBuf>,
}

/// Executes the init command.
pub(crate) async fn execute(args: &InitArgs) -> Result<(), Box<dyn std::error::Error>> {
	let project_dir = args.dir.clone().unwrap_or_else(|| PathBuf::from("."));

	// Check if nuages.toml already exists
	let nuages_toml_path = project_dir.join("nuages.toml");
	if nuages_toml_path.exists() {
		return Err("nuages.toml already exists. Use `nuages sync` to update.".into());
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

	// Generate config
	let config = generate_config(&metadata, db_config.as_ref());
	let toml_string = generate_nuages_toml_string(&config);

	// Write nuages.toml
	std::fs::write(&nuages_toml_path, &toml_string)?;
	println!("\nGenerated nuages.toml");

	Ok(())
}
