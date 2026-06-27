//! Dockerfile auto-generation for reinhardt-web applications.

mod cargo_lock_reader;
mod dockerfile;
mod rust_toolchain_reader;
mod stages;

use std::path::{Path, PathBuf};

use reinhardt_cloud_types::reinhardt_cloud_toml::ReinhardtCloudToml;

/// Walk from `start_dir` upward looking for a file with the given name.
///
/// The search is bounded to the detected Cargo workspace root. This keeps
/// workspace-level files (`Cargo.lock`, `rust-toolchain.toml`) discoverable
/// from member crate directories without trusting arbitrary files placed in
/// shared ancestor directories above the workspace. Standalone projects only
/// trust files in their own project directory.
fn locate_workspace_file(start_dir: &Path, file_name: &str) -> Option<PathBuf> {
	let start = start_dir.canonicalize().ok()?;
	let boundary = locate_workspace_boundary(&start).unwrap_or_else(|| start.clone());
	let mut current = start;
	loop {
		let candidate = current.join(file_name);
		if candidate.is_file() {
			return Some(candidate);
		}
		if current == boundary {
			return None;
		}
		current = current.parent()?.to_path_buf();
	}
}

fn locate_workspace_boundary(start_dir: &Path) -> Option<PathBuf> {
	let mut current = start_dir.to_path_buf();
	loop {
		let manifest = current.join("Cargo.toml");
		if manifest.is_file()
			&& let Ok(content) = std::fs::read_to_string(&manifest)
			&& let Ok(parsed) = toml::from_str::<toml::Value>(&content)
			&& parsed.get("workspace").is_some()
		{
			return Some(current);
		}
		current = current.parent()?.to_path_buf();
	}
}

pub(crate) use self::dockerfile::Dockerfile;
pub(crate) use self::stages::DockerfileSignals;

use self::stages::{build_builder_stage, build_chef_stage, build_runtime_stage, build_wasm_stage};

/// Reason why Dockerfile generation was skipped.
#[derive(Debug, PartialEq)]
pub(crate) enum SkipReason {
	/// No skip — generate the Dockerfile.
	None,
	/// User has a custom Dockerfile path in `[source.build].dockerfile`.
	CustomDockerfile,
	/// Dockerfile already exists and `--force` was not specified.
	AlreadyExists,
}

/// Returns `true` if the path refers to the default Dockerfile location.
fn is_default_dockerfile_path(path: &str) -> bool {
	matches!(path, "Dockerfile" | "./Dockerfile")
}

/// Check whether Dockerfile generation should be skipped.
pub(crate) fn should_skip_dockerfile(
	project_dir: &Path,
	toml_config: &ReinhardtCloudToml,
	force: bool,
) -> SkipReason {
	// Check custom dockerfile path (skip default names)
	if let Some(source) = &toml_config.source
		&& let Some(build) = &source.build
		&& let Some(df) = &build.dockerfile
		&& !df.is_empty()
		&& !is_default_dockerfile_path(df)
	{
		return SkipReason::CustomDockerfile;
	}

	// Check if Dockerfile already exists
	if project_dir.join("Dockerfile").exists() && !force {
		return SkipReason::AlreadyExists;
	}

	SkipReason::None
}

/// Collect all signals needed for Dockerfile generation.
pub(crate) fn collect_signals(
	project_dir: &Path,
	metadata: &crate::feature_detector::ProjectMetadata,
	toml_config: &ReinhardtCloudToml,
) -> Result<DockerfileSignals, String> {
	let rust_version = rust_toolchain_reader::read_rust_version(project_dir)?;

	let signals = &metadata.signals;

	// Read Cargo.lock once and share its content between every reader that
	// needs to walk the resolved dependency graph (wasm-bindgen version
	// resolution, protoc requirement detection, ...). Cargo.lock lives at
	// the workspace root in workspace projects, not in the member crate,
	// so walk up if not found locally.
	let cargo_lock_content: Option<String> =
		if let Some(lock_path) = locate_workspace_file(project_dir, "Cargo.lock") {
			Some(
				std::fs::read_to_string(&lock_path)
					.map_err(|e| format!("failed to read {}: {e}", lock_path.display()))?,
			)
		} else {
			None
		};

	// wasm-bindgen version: Cargo.lock > reinhardt-cloud.toml build_args.
	let wasm_bindgen_version = if signals.pages {
		let from_lock = match &cargo_lock_content {
			Some(content) => cargo_lock_reader::extract_wasm_bindgen_version(content)?,
			None => None,
		};

		let version = from_lock.or_else(|| {
			// Fallback: check reinhardt-cloud.toml build_args
			toml_config
				.source
				.as_ref()
				.and_then(|s| s.build.as_ref())
				.and_then(|b| b.build_args.get("WASM_BINDGEN_VERSION").cloned())
		});

		let Some(version) = version else {
			return Err("pages feature detected but wasm-bindgen version not found \
				 in Cargo.lock or reinhardt-cloud.toml"
				.to_string());
		};
		validate_docker_token(&version, "wasm-bindgen version")?;
		Some(version)
	} else {
		None
	};

	// protoc requirement: detected from Cargo.lock so that transitive
	// prost/tonic dependencies (e.g., reinhardt-cloud-grpc pulling in
	// tonic-build) trigger installation even when the consumer crate does
	// not opt into the reinhardt-web `grpc` feature flag.
	let protoc_needed = match &cargo_lock_content {
		Some(content) => cargo_lock_reader::detect_protoc_requirement(content),
		None => false,
	};

	// base_image override
	let base_image_override = toml_config
		.source
		.as_ref()
		.and_then(|s| s.build.as_ref())
		.and_then(|b| b.base_image.clone());

	// Detect `settings/` for deployment metadata only. Runtime images must not
	// copy this directory because settings TOMLs can contain secrets.
	let has_settings_dir = project_dir.join("settings").is_dir();
	let has_migrations_dir = project_dir.join("migrations").is_dir();

	// Compute the project's path relative to the Docker build context.
	// The build context is the workspace root (where `Cargo.lock` lives
	// in workspace projects). For single-crate projects, project_dir
	// IS the workspace root, so the relative path is empty and we use
	// `None` to signal "no prefix needed".
	let project_relative_path = compute_project_relative_path(project_dir);

	Ok(DockerfileSignals {
		project_name: metadata.name.clone(),
		rust_version,
		pages: signals.pages,
		grpc: signals.grpc,
		graphql: signals.graphql,
		wasm_bindgen_version,
		database: signals.database.clone(),
		cache: signals.cache.clone(),
		session_backend: None, // Only available via introspect at deploy time
		base_image_override,
		tracing: signals.tracing,
		protoc_needed,
		has_settings_dir,
		has_migrations_dir,
		project_relative_path,
	})
}

/// Returns the project's path relative to the workspace root (Docker
/// build context), or `None` if the project IS the workspace root or
/// the relationship cannot be determined. Helper extracted from
/// `collect_signals` for testability.
fn compute_project_relative_path(project_dir: &Path) -> Option<String> {
	let lock_path = locate_workspace_file(project_dir, "Cargo.lock")?;
	let workspace_root = lock_path.parent()?;
	let project_canonical = project_dir.canonicalize().ok()?;
	let workspace_canonical = workspace_root.canonicalize().ok()?;
	if project_canonical == workspace_canonical {
		return None;
	}
	let rel = project_canonical
		.strip_prefix(&workspace_canonical)
		.ok()?
		.to_string_lossy()
		.into_owned();
	if rel.is_empty() { None } else { Some(rel) }
}

fn validate_docker_token(value: &str, label: &str) -> Result<(), String> {
	if value.is_empty()
		|| !value
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
	{
		return Err(format!(
			"invalid {label}: expected a Dockerfile-safe token containing only ASCII letters, digits, '.', '-', '_', or '+'"
		));
	}
	Ok(())
}

/// Generate a Dockerfile from signals.
pub(crate) fn generate(signals: &DockerfileSignals) -> Dockerfile {
	let mut stages = vec![build_chef_stage(signals), build_builder_stage(signals)];

	if signals.pages {
		stages.push(build_wasm_stage(signals));
	}

	stages.push(build_runtime_stage(signals));

	Dockerfile {
		header_comment: "Generated by reinhardt-cloud. Do not edit manually.".to_string(),
		stages,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use reinhardt_cloud_types::reinhardt_cloud_toml::{BuildSection, SourceSection};
	use rstest::*;

	fn minimal_signals() -> DockerfileSignals {
		DockerfileSignals {
			project_name: "my-app".to_string(),
			rust_version: "1.94.1".to_string(),
			pages: false,
			grpc: false,
			graphql: false,
			wasm_bindgen_version: None,
			database: None,
			cache: None,
			session_backend: None,
			base_image_override: None,
			tracing: false,
			protoc_needed: false,
			has_settings_dir: false,
			has_migrations_dir: false,
			project_relative_path: None,
		}
	}

	fn config_with_source_build(build: Option<BuildSection>) -> ReinhardtCloudToml {
		ReinhardtCloudToml {
			source: Some(SourceSection {
				repository: String::new(),
				build,
				..Default::default()
			}),
			..Default::default()
		}
	}

	// G1
	#[rstest]
	fn snapshot_api_minimal() {
		let df = generate(&minimal_signals());
		assert_eq!(
			df.to_string(),
			include_str!("dockerfile_generator/snapshots/api_minimal.dockerfile")
		);
	}

	// G2
	#[rstest]
	fn snapshot_api_postgres() {
		let signals = DockerfileSignals {
			database: Some("postgresql".into()),
			..minimal_signals()
		};
		assert_eq!(
			generate(&signals).to_string(),
			include_str!("dockerfile_generator/snapshots/api_postgres.dockerfile")
		);
	}

	// G3
	#[rstest]
	fn snapshot_api_mysql() {
		let signals = DockerfileSignals {
			database: Some("mysql".into()),
			..minimal_signals()
		};
		assert_eq!(
			generate(&signals).to_string(),
			include_str!("dockerfile_generator/snapshots/api_mysql.dockerfile")
		);
	}

	// G4
	#[rstest]
	fn snapshot_api_sqlite() {
		let signals = DockerfileSignals {
			database: Some("sqlite".into()),
			..minimal_signals()
		};
		assert_eq!(
			generate(&signals).to_string(),
			include_str!("dockerfile_generator/snapshots/api_sqlite.dockerfile")
		);
	}

	// G5
	#[rstest]
	fn snapshot_api_grpc() {
		let signals = DockerfileSignals {
			grpc: true,
			..minimal_signals()
		};
		assert_eq!(
			generate(&signals).to_string(),
			include_str!("dockerfile_generator/snapshots/api_grpc.dockerfile")
		);
	}

	// G6
	#[rstest]
	fn snapshot_api_graphql() {
		let signals = DockerfileSignals {
			graphql: true,
			..minimal_signals()
		};
		assert_eq!(
			generate(&signals).to_string(),
			include_str!("dockerfile_generator/snapshots/api_graphql.dockerfile")
		);
	}

	// G7
	#[rstest]
	fn snapshot_api_full() {
		let signals = DockerfileSignals {
			database: Some("postgresql".into()),
			grpc: true,
			graphql: true,
			..minimal_signals()
		};
		assert_eq!(
			generate(&signals).to_string(),
			include_str!("dockerfile_generator/snapshots/api_full.dockerfile")
		);
	}

	// G8
	#[rstest]
	fn snapshot_pages_minimal() {
		let signals = DockerfileSignals {
			pages: true,
			wasm_bindgen_version: Some("0.2.100".into()),
			..minimal_signals()
		};
		assert_eq!(
			generate(&signals).to_string(),
			include_str!("dockerfile_generator/snapshots/pages_minimal.dockerfile")
		);
	}

	// G9
	#[rstest]
	fn snapshot_pages_postgres() {
		let signals = DockerfileSignals {
			pages: true,
			wasm_bindgen_version: Some("0.2.100".into()),
			database: Some("postgresql".into()),
			..minimal_signals()
		};
		assert_eq!(
			generate(&signals).to_string(),
			include_str!("dockerfile_generator/snapshots/pages_postgres.dockerfile")
		);
	}

	// G10
	#[rstest]
	fn snapshot_pages_full() {
		let signals = DockerfileSignals {
			pages: true,
			wasm_bindgen_version: Some("0.2.100".into()),
			database: Some("postgresql".into()),
			grpc: true,
			graphql: true,
			..minimal_signals()
		};
		assert_eq!(
			generate(&signals).to_string(),
			include_str!("dockerfile_generator/snapshots/pages_full.dockerfile")
		);
	}

	// G10b: pages project shipping a `settings/` directory that lives at
	// `dashboard/` inside the workspace. The runtime stage must not bundle
	// settings TOMLs because they can contain deployment secrets.
	#[rstest]
	fn snapshot_pages_with_settings() {
		let signals = DockerfileSignals {
			pages: true,
			wasm_bindgen_version: Some("0.2.100".into()),
			database: Some("postgresql".into()),
			has_settings_dir: true,
			has_migrations_dir: true,
			project_relative_path: Some("dashboard".to_string()),
			..minimal_signals()
		};
		assert_eq!(
			generate(&signals).to_string(),
			include_str!("dockerfile_generator/snapshots/pages_with_settings.dockerfile")
		);
	}

	// G11
	#[rstest]
	fn snapshot_custom_base_image() {
		let signals = DockerfileSignals {
			base_image_override: Some("gcr.io/distroless/cc-debian12".into()),
			..minimal_signals()
		};
		let df = generate(&signals);
		let output = df.to_string();
		assert!(output.contains("FROM gcr.io/distroless/cc-debian12 AS runtime"));
		assert!(!output.contains("debian:bookworm-slim"));
		// Custom image: no Debian-specific commands
		assert!(!output.contains("apt-get"));
		assert!(!output.contains("useradd"));
		assert!(!output.contains("tini"));
	}

	// G12
	#[rstest]
	fn snapshot_cockroachdb_fallback() {
		let signals = DockerfileSignals {
			database: Some("cockroachdb".into()),
			..minimal_signals()
		};
		let df = generate(&signals);
		assert!(df.to_string().contains("libpq5"));
	}

	// SK1
	#[rstest]
	fn skip_when_custom_dockerfile_set() {
		// Arrange
		let dir = std::env::temp_dir();
		let config = config_with_source_build(Some(BuildSection {
			dockerfile: Some("docker/Dockerfile.prod".to_string()),
			..Default::default()
		}));

		// Act
		let result = should_skip_dockerfile(&dir, &config, false);

		// Assert
		assert_eq!(result, SkipReason::CustomDockerfile);
	}

	// SK2
	#[rstest]
	fn skip_when_exists_no_force() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(dir.path().join("Dockerfile"), "FROM scratch").unwrap();
		let config = ReinhardtCloudToml::default();

		// Act
		let result = should_skip_dockerfile(dir.path(), &config, false);

		// Assert
		assert_eq!(result, SkipReason::AlreadyExists);
	}

	// SK3
	#[rstest]
	fn no_skip_when_force() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(dir.path().join("Dockerfile"), "FROM scratch").unwrap();
		let config = ReinhardtCloudToml::default();

		// Act
		let result = should_skip_dockerfile(dir.path(), &config, true);

		// Assert
		assert_eq!(result, SkipReason::None);
	}

	// SK4
	#[rstest]
	fn no_skip_fresh_project() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		let config = ReinhardtCloudToml::default();

		// Act
		let result = should_skip_dockerfile(dir.path(), &config, false);

		// Assert
		assert_eq!(result, SkipReason::None);
	}

	// SK5
	#[rstest]
	fn custom_dockerfile_empty_string() {
		// Arrange
		let dir = std::env::temp_dir();
		let config = config_with_source_build(Some(BuildSection {
			dockerfile: Some(String::new()),
			..Default::default()
		}));

		// Act
		let result = should_skip_dockerfile(&dir, &config, false);

		// Assert
		assert_eq!(result, SkipReason::None);
	}

	// SK6
	#[rstest]
	fn source_section_missing() {
		let dir = std::env::temp_dir();
		let config = ReinhardtCloudToml::default();
		assert_eq!(
			should_skip_dockerfile(&dir, &config, false),
			SkipReason::None
		);
	}

	// SK7a
	#[rstest]
	fn no_skip_when_default_dockerfile_path() {
		// Arrange
		let dir = std::env::temp_dir();
		let config = config_with_source_build(Some(BuildSection {
			dockerfile: Some("Dockerfile".to_string()),
			..Default::default()
		}));

		// Act
		let result = should_skip_dockerfile(&dir, &config, false);

		// Assert
		assert_eq!(result, SkipReason::None);
	}

	// SK7b
	#[rstest]
	fn no_skip_when_dot_slash_dockerfile_path() {
		// Arrange
		let dir = std::env::temp_dir();
		let config = config_with_source_build(Some(BuildSection {
			dockerfile: Some("./Dockerfile".to_string()),
			..Default::default()
		}));

		// Act
		let result = should_skip_dockerfile(&dir, &config, false);

		// Assert
		assert_eq!(result, SkipReason::None);
	}

	// SK8
	#[rstest]
	fn build_section_missing() {
		// Arrange
		let dir = std::env::temp_dir();
		let config = config_with_source_build(None);

		// Act
		let result = should_skip_dockerfile(&dir, &config, false);

		// Assert
		assert_eq!(result, SkipReason::None);
	}

	#[rstest]
	fn workspace_file_lookup_stops_at_workspace_root() {
		// Arrange
		let outer_dir = tempfile::tempdir().unwrap();
		std::fs::write(outer_dir.path().join("Cargo.lock"), "malicious lock").unwrap();
		let workspace_dir = outer_dir.path().join("workspace");
		let member_dir = workspace_dir.join("dashboard");
		std::fs::create_dir_all(&member_dir).unwrap();
		std::fs::write(
			workspace_dir.join("Cargo.toml"),
			"[workspace]\nmembers = [\"dashboard\"]\n",
		)
		.unwrap();

		// Act
		let result = locate_workspace_file(&member_dir, "Cargo.lock");

		// Assert
		assert_eq!(result, None);
	}

	#[rstest]
	fn workspace_file_lookup_finds_workspace_root_file() {
		// Arrange
		let workspace_dir = tempfile::tempdir().unwrap();
		let member_dir = workspace_dir.path().join("dashboard");
		std::fs::create_dir(&member_dir).unwrap();
		std::fs::write(
			workspace_dir.path().join("Cargo.toml"),
			"[workspace]\nmembers = [\"dashboard\"]\n",
		)
		.unwrap();
		let lock_path = workspace_dir.path().join("Cargo.lock");
		std::fs::write(&lock_path, "workspace lock").unwrap();

		// Act
		let result = locate_workspace_file(&member_dir, "Cargo.lock");

		// Assert
		assert_eq!(result, Some(lock_path));
	}

	/// Integration test (Refs #477): a Cargo.lock that pulls in `prost-build`
	/// or `tonic-build` (directly or transitively) must propagate through
	/// `collect_signals` so the generated Dockerfile installs
	/// `protobuf-compiler` in both the chef and builder stages.
	#[rstest]
	fn collect_signals_propagates_protoc_from_cargo_lock() {
		// Arrange — minimal workspace fixture: rust-toolchain.toml,
		// Cargo.lock with a tonic-build entry, and a project metadata
		// stand-in that does NOT enable the reinhardt-web `grpc` feature.
		// This proves the lockfile path is the one driving protoc install.
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("rust-toolchain.toml"),
			r#"
[toolchain]
channel = "1.94.1"
"#,
		)
		.unwrap();
		std::fs::write(
			dir.path().join("Cargo.lock"),
			r#"
[[package]]
name = "serde"
version = "1.0.200"

[[package]]
name = "tonic-build"
version = "0.13.0"
"#,
		)
		.unwrap();

		let metadata = crate::feature_detector::ProjectMetadata {
			name: "consumer-app".to_string(),
			version: "0.1.0".to_string(),
			features: vec![],
			signals: crate::feature_detector::InfraSignals::default(),
		};
		let toml_config = ReinhardtCloudToml::default();

		// Act
		let signals = collect_signals(dir.path(), &metadata, &toml_config)
			.expect("collect_signals must succeed");
		let dockerfile = generate(&signals).to_string();

		// Assert
		assert!(
			signals.protoc_needed,
			"protoc_needed must be true when Cargo.lock carries tonic-build"
		);
		assert!(
			!signals.grpc,
			"grpc must remain false: protoc detection must NOT depend on the grpc feature flag"
		);
		// Both build stages must install the compiler.
		let chef_install_count = dockerfile
			.split("FROM rust:")
			.nth(1)
			.expect("chef stage")
			.matches("protobuf-compiler")
			.count();
		let builder_install_count = dockerfile
			.split("FROM rust:")
			.nth(2)
			.expect("builder stage")
			.matches("protobuf-compiler")
			.count();
		assert!(
			chef_install_count >= 1,
			"chef stage must contain protobuf-compiler install, got dockerfile:\n{dockerfile}"
		);
		assert!(
			builder_install_count >= 1,
			"builder stage must contain protobuf-compiler install, got dockerfile:\n{dockerfile}"
		);
	}

	/// Integration test (Refs #477): a Cargo.lock without prost/tonic must
	/// keep the slim Dockerfile baseline — no spurious protoc install.
	#[rstest]
	fn collect_signals_omits_protoc_for_unrelated_lockfile() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("rust-toolchain.toml"),
			r#"
[toolchain]
channel = "1.94.1"
"#,
		)
		.unwrap();
		std::fs::write(
			dir.path().join("Cargo.lock"),
			r#"
[[package]]
name = "serde"
version = "1.0.200"

[[package]]
name = "tokio"
version = "1.40.0"
"#,
		)
		.unwrap();

		let metadata = crate::feature_detector::ProjectMetadata {
			name: "plain-app".to_string(),
			version: "0.1.0".to_string(),
			features: vec![],
			signals: crate::feature_detector::InfraSignals::default(),
		};
		let toml_config = ReinhardtCloudToml::default();

		// Act
		let signals = collect_signals(dir.path(), &metadata, &toml_config)
			.expect("collect_signals must succeed");
		let dockerfile = generate(&signals).to_string();

		// Assert
		assert!(
			!signals.protoc_needed,
			"protoc_needed must be false when Cargo.lock has no prost/tonic"
		);
		assert!(
			!dockerfile.contains("protobuf-compiler"),
			"Dockerfile must not install protobuf-compiler for unrelated dependency tree"
		);
	}
}
