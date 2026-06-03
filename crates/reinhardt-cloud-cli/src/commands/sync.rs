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
	let existing_toml = std::fs::read_to_string(&reinhardt_cloud_toml_path)?;
	let existing_config: reinhardt_cloud_types::reinhardt_cloud_toml::ReinhardtCloudToml =
		toml::from_str(&existing_toml)?;
	let metadata = detect_project(&project_dir)?;
	let db_config = read_database_config(&project_dir);
	let mut config = generate_config(&metadata, db_config.as_ref())?;
	merge_existing_infrastructure(&existing_config, &mut config);
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

fn merge_existing_infrastructure(
	existing: &reinhardt_cloud_types::reinhardt_cloud_toml::ReinhardtCloudToml,
	generated: &mut reinhardt_cloud_types::reinhardt_cloud_toml::ReinhardtCloudToml,
) {
	let Some(existing_infrastructure) = existing.infrastructure.as_ref() else {
		return;
	};

	let Some(generated_infrastructure) = generated.infrastructure.as_mut() else {
		generated.infrastructure = Some(existing_infrastructure.clone());
		return;
	};

	if let Some(postgres) = existing_infrastructure.postgres.as_ref() {
		generated_infrastructure.postgres = Some(postgres.clone());
	}
	if let Some(buckets) = existing_infrastructure.buckets.as_ref() {
		generated_infrastructure.buckets = Some(buckets.clone());
	}
	if let Some(dns) = existing_infrastructure.dns.as_ref() {
		generated_infrastructure.dns = Some(dns.clone());
	}
	if let Some(secrets) = existing_infrastructure.secrets.as_ref() {
		generated_infrastructure.secrets = Some(secrets.clone());
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use reinhardt_cloud_types::reinhardt_cloud_toml::ReinhardtCloudToml;

	#[test]
	fn merge_preserves_existing_infrastructure_sections() {
		let existing: ReinhardtCloudToml = toml::from_str(
			r#"
[app]
name = "inventory"
image = "inventory:latest"

[infrastructure.postgres]
tier = "db-custom-2-4096"
version = "15"
backup_retention_days = 14
"#,
		)
		.expect("existing config should parse");
		let mut generated: ReinhardtCloudToml = toml::from_str(
			r#"
[app]
name = "inventory"
image = "inventory:latest"

[infrastructure.postgres]
version = "16"

[[infrastructure.buckets]]
name = "inventory-assets"
public = false
"#,
		)
		.expect("generated config should parse");

		merge_existing_infrastructure(&existing, &mut generated);

		let infrastructure = generated
			.infrastructure
			.expect("merged config should retain infrastructure");
		let postgres = infrastructure
			.postgres
			.expect("existing postgres should be preserved");
		assert_eq!(postgres.tier.as_deref(), Some("db-custom-2-4096"));
		assert_eq!(postgres.version.as_deref(), Some("15"));
		assert_eq!(postgres.backup_retention_days, Some(14));
		let buckets = infrastructure
			.buckets
			.expect("generated buckets should remain when existing has none");
		assert_eq!(buckets.len(), 1);
		assert_eq!(buckets[0].name, "inventory-assets");
		assert!(!buckets[0].public);
	}

	#[test]
	fn merge_copies_existing_infrastructure_when_generated_has_none() {
		let existing: ReinhardtCloudToml = toml::from_str(
			r#"
[app]
name = "inventory"
image = "inventory:latest"

[[infrastructure.buckets]]
name = "inventory-uploads"
public = true
"#,
		)
		.expect("existing config should parse");
		let mut generated: ReinhardtCloudToml = toml::from_str(
			r#"
[app]
name = "inventory"
image = "inventory:latest"
"#,
		)
		.expect("generated config should parse");

		merge_existing_infrastructure(&existing, &mut generated);

		let infrastructure = generated
			.infrastructure
			.expect("existing infrastructure should be copied");
		let buckets = infrastructure
			.buckets
			.expect("existing buckets should be copied");
		assert_eq!(buckets.len(), 1);
		assert_eq!(buckets[0].name, "inventory-uploads");
		assert!(buckets[0].public);
	}
}
