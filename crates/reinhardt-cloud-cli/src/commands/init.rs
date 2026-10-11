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

	// Generate reinhardt-cloud.toml and resolve the Dockerfile before writing
	// either, so a failed generation cannot leave the deployment config
	// describing an image that was not built.
	let mut config = generate_config(&metadata, db_config.as_ref())?;
	dockerfile_generator::configure_pages(&project_dir, &metadata, &mut config, args.force)?;
	let skip = dockerfile_generator::should_skip_dockerfile(&project_dir, &config, args.force);
	let generated = match skip {
		SkipReason::None => {
			let signals = dockerfile_generator::collect_signals(&project_dir, &metadata, &config)?;
			let dockerfile = dockerfile_generator::generate(&signals);
			Some((signals, dockerfile))
		}
		SkipReason::CustomDockerfile | SkipReason::AlreadyExists => None,
	};
	let toml_string = generate_reinhardt_cloud_toml_string(&config);
	let dockerfile_path = project_dir.join("Dockerfile");
	let dockerfile_string = generated
		.as_ref()
		.map(|(_, dockerfile)| dockerfile.to_string());

	// Replace both files together so they never describe different images.
	dockerfile_generator::write_project_files(
		&reinhardt_cloud_toml_path,
		&toml_string,
		dockerfile_string
			.as_deref()
			.map(|contents| (dockerfile_path.as_path(), contents)),
	)?;
	println!("Created reinhardt-cloud.toml");

	match generated {
		Some((signals, _)) => {
			let pattern = if signals.pages { "pages" } else { "api" };
			let db_info = signals
				.database
				.as_deref()
				.map(|d| format!(" + {d}"))
				.unwrap_or_default();
			println!("Created Dockerfile ({pattern}{db_info})");
		}
		None if skip == SkipReason::CustomDockerfile => {
			println!("Skipped Dockerfile (custom path set in [source.build])");
		}
		None => {
			println!("Skipped Dockerfile (already exists — use --force to overwrite)");
		}
	}

	Ok(())
}
