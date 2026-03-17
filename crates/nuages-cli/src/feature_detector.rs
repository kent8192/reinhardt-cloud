//! Detects reinhardt-web features from `Cargo.toml` and maps to infrastructure signals.

use std::path::Path;

/// Infrastructure signals inferred from reinhardt-web feature flags
#[derive(Debug, Clone, Default)]
pub(crate) struct InfraSignals {
	pub(crate) database: Option<String>,
	pub(crate) jwt: bool,
	pub(crate) websocket: bool,
	pub(crate) background_worker: bool,
	pub(crate) cache: Option<String>,
	pub(crate) grpc: bool,
	pub(crate) object_storage: bool,
	pub(crate) sessions: bool,
}

impl InfraSignals {
	/// Infer infrastructure signals from resolved feature flags
	pub(crate) fn from_features(features: &[String]) -> Self {
		let has = |f: &str| features.iter().any(|feat| feat == f);
		Self {
			database: if has("db-postgres") {
				Some("postgresql".to_owned())
			} else if has("db-mysql") {
				Some("mysql".to_owned())
			} else if has("db-sqlite") {
				Some("sqlite".to_owned())
			} else if has("database") {
				// Generic database feature defaults to postgresql
				Some("postgresql".to_owned())
			} else {
				None
			},
			jwt: has("auth-jwt"),
			websocket: has("websockets"),
			background_worker: has("tasks"),
			cache: if has("redis-backend") {
				Some("redis".to_owned())
			} else {
				None
			},
			grpc: has("grpc"),
			object_storage: has("storage") || has("static-files"),
			sessions: has("sessions"),
		}
	}
}

/// Project metadata extracted from `Cargo.toml`
#[derive(Debug, Clone)]
pub(crate) struct ProjectMetadata {
	pub(crate) name: String,
	pub(crate) version: String,
	pub(crate) features: Vec<String>,
	pub(crate) signals: InfraSignals,
}

/// Detect reinhardt-web project from `Cargo.toml` at the given path
pub(crate) fn detect_project(project_dir: &Path) -> Result<ProjectMetadata, String> {
	let cargo_toml_path = project_dir.join("Cargo.toml");
	let content = std::fs::read_to_string(&cargo_toml_path)
		.map_err(|e| format!("Failed to read Cargo.toml: {e}"))?;
	let parsed: toml::Value =
		toml::from_str(&content).map_err(|e| format!("Failed to parse Cargo.toml: {e}"))?;

	// Extract package name
	let name = parsed
		.get("package")
		.and_then(|p| p.get("name"))
		.and_then(|n| n.as_str())
		.ok_or("Missing [package].name in Cargo.toml")?
		.to_owned();

	let version = parsed
		.get("package")
		.and_then(|p| p.get("version"))
		.and_then(|v| v.as_str())
		.unwrap_or("0.1.0")
		.to_owned();

	// Find reinhardt-web dependency (handles package rename pattern)
	let features = find_reinhardt_features(&parsed)
		.ok_or("Not a reinhardt-web project: no reinhardt-web dependency found")?;

	let signals = InfraSignals::from_features(&features);

	Ok(ProjectMetadata {
		name,
		version,
		features,
		signals,
	})
}

/// Search for reinhardt-web dependency features across dependency sections
fn find_reinhardt_features(cargo_toml: &toml::Value) -> Option<Vec<String>> {
	// Check [dependencies] first
	if let Some(deps) = cargo_toml.get("dependencies") {
		if let Some(features) = extract_reinhardt_features(deps) {
			return Some(features);
		}
	}
	// Check [workspace.dependencies]
	if let Some(workspace) = cargo_toml.get("workspace") {
		if let Some(deps) = workspace.get("dependencies") {
			if let Some(features) = extract_reinhardt_features(deps) {
				return Some(features);
			}
		}
	}
	None
}

/// Extract features from deps table, checking for both direct and renamed dependency keys
fn extract_reinhardt_features(deps: &toml::Value) -> Option<Vec<String>> {
	// Check for key "reinhardt-web" directly
	if let Some(dep) = deps.get("reinhardt-web") {
		return extract_features_from_dep(dep);
	}
	// Check all deps for package = "reinhardt-web" (rename pattern)
	if let Some(table) = deps.as_table() {
		for (_key, dep) in table {
			if let Some(pkg) = dep.get("package").and_then(|p| p.as_str()) {
				if pkg == "reinhardt-web" {
					return extract_features_from_dep(dep);
				}
			}
		}
	}
	None
}

/// Extract the features array from a single dependency value.
///
/// Returns `Some` only for table-style dependencies (inline or expanded).
/// String-style dependencies (e.g., `reinhardt-web = "0.1"`) return `None`
/// since they cannot carry feature flags.
fn extract_features_from_dep(dep: &toml::Value) -> Option<Vec<String>> {
	// Only table-style deps can have features
	if !dep.is_table() {
		return None;
	}
	Some(
		dep.get("features")
			.and_then(|f| f.as_array())
			.map(|arr| {
				arr.iter()
					.filter_map(|v| v.as_str().map(String::from))
					.collect()
			})
			.unwrap_or_default(),
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_infer_signals_from_features() {
		// Arrange
		let features = vec!["db-postgres".into(), "auth-jwt".into(), "websockets".into()];

		// Act
		let signals = InfraSignals::from_features(&features);

		// Assert
		assert_eq!(signals.database, Some("postgresql".to_owned()));
		assert!(signals.jwt);
		assert!(signals.websocket);
		assert!(!signals.background_worker);
	}

	#[rstest]
	fn test_infer_signals_minimal() {
		// Arrange
		let features = vec!["core".into(), "server".into()];

		// Act
		let signals = InfraSignals::from_features(&features);

		// Assert
		assert_eq!(signals.database, None);
		assert!(!signals.jwt);
	}

	#[rstest]
	fn test_infer_signals_mysql() {
		// Arrange
		let features = vec!["db-mysql".into()];

		// Act
		let signals = InfraSignals::from_features(&features);

		// Assert
		assert_eq!(signals.database, Some("mysql".to_owned()));
	}

	#[rstest]
	fn test_infer_signals_redis_cache() {
		// Arrange
		let features = vec!["redis-backend".into()];

		// Act
		let signals = InfraSignals::from_features(&features);

		// Assert
		assert_eq!(signals.cache, Some("redis".to_owned()));
	}

	#[rstest]
	fn test_infer_signals_background_worker() {
		// Arrange
		let features = vec!["tasks".into()];

		// Act
		let signals = InfraSignals::from_features(&features);

		// Assert
		assert!(signals.background_worker);
	}

	#[rstest]
	fn test_infer_signals_object_storage() {
		// Arrange
		let features = vec!["storage".into()];

		// Act
		let signals = InfraSignals::from_features(&features);

		// Assert
		assert!(signals.object_storage);
	}

	#[rstest]
	fn test_infer_signals_static_files_triggers_storage() {
		// Arrange
		let features = vec!["static-files".into()];

		// Act
		let signals = InfraSignals::from_features(&features);

		// Assert
		assert!(signals.object_storage);
	}

	#[rstest]
	fn test_detect_project_with_package_rename() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			r#"
[package]
name = "test-app"
version = "0.2.0"

[dependencies]
reinhardt = { package = "reinhardt-web", version = "0.1", features = ["standard", "auth-jwt"] }
"#,
		)
		.unwrap();

		// Act
		let result = detect_project(dir.path());

		// Assert
		let metadata = result.unwrap();
		assert_eq!(metadata.name, "test-app");
		assert_eq!(metadata.version, "0.2.0");
		assert!(metadata.features.contains(&"standard".to_owned()));
		assert!(metadata.features.contains(&"auth-jwt".to_owned()));
	}

	#[rstest]
	fn test_detect_project_direct_dependency() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			r#"
[package]
name = "direct-app"
version = "1.0.0"

[dependencies]
reinhardt-web = { version = "0.1", features = ["db-postgres", "sessions"] }
"#,
		)
		.unwrap();

		// Act
		let result = detect_project(dir.path());

		// Assert
		let metadata = result.unwrap();
		assert_eq!(metadata.name, "direct-app");
		assert!(metadata.features.contains(&"db-postgres".to_owned()));
		assert!(metadata.features.contains(&"sessions".to_owned()));
		assert_eq!(metadata.signals.database, Some("postgresql".to_owned()));
		assert!(metadata.signals.sessions);
	}

	#[rstest]
	fn test_detect_project_workspace_dependency() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			r#"
[package]
name = "ws-app"
version = "0.3.0"

[workspace.dependencies]
reinhardt-web = { version = "0.1", features = ["db-postgres", "auth-jwt", "tasks"] }
"#,
		)
		.unwrap();

		// Act
		let result = detect_project(dir.path());

		// Assert
		let metadata = result.unwrap();
		assert_eq!(metadata.name, "ws-app");
		assert!(metadata.signals.jwt);
		assert!(metadata.signals.background_worker);
	}

	#[rstest]
	fn test_detect_project_not_reinhardt() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			r#"
[package]
name = "other-app"
version = "0.1.0"

[dependencies]
tokio = "1"
"#,
		)
		.unwrap();

		// Act
		let result = detect_project(dir.path());

		// Assert
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("Not a reinhardt-web project"));
	}

	#[rstest]
	fn test_detect_project_no_cargo_toml() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();

		// Act
		let result = detect_project(dir.path());

		// Assert
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("Failed to read Cargo.toml"));
	}

	#[rstest]
	fn test_detect_project_workspace_dependency_features() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			r#"
[package]
name = "workspace-app"
version = "1.0.0"

[workspace.dependencies]
reinhardt-web = { version = "0.1", features = ["db-postgres", "redis-backend", "sessions"] }
"#,
		)
		.unwrap();

		// Act
		let result = detect_project(dir.path());

		// Assert
		let metadata = result.unwrap();
		assert_eq!(metadata.name, "workspace-app");
		assert_eq!(metadata.signals.database, Some("postgresql".to_owned()));
		assert_eq!(metadata.signals.cache, Some("redis".to_owned()));
		assert!(metadata.signals.sessions);
	}

	#[rstest]
	fn test_detect_project_no_dependencies_section() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			r#"
[package]
name = "empty-deps"
version = "0.1.0"
"#,
		)
		.unwrap();

		// Act
		let result = detect_project(dir.path());

		// Assert
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("Not a reinhardt-web project"));
	}

	#[rstest]
	fn test_detect_project_reinhardt_web_table_dep_without_features() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			r#"
[package]
name = "no-feat-app"
version = "0.1.0"

[dependencies]
reinhardt-web = { version = "0.1" }
"#,
		)
		.unwrap();

		// Act
		let result = detect_project(dir.path());

		// Assert
		let metadata = result.unwrap();
		assert_eq!(metadata.name, "no-feat-app");
		assert!(metadata.features.is_empty());
		assert!(metadata.signals.database.is_none());
	}

	#[rstest]
	fn test_detect_project_invalid_toml() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(dir.path().join("Cargo.toml"), "invalid {{{ toml").unwrap();

		// Act
		let result = detect_project(dir.path());

		// Assert
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("Failed to parse Cargo.toml"));
	}

	#[rstest]
	fn test_infer_signals_full_bundle() {
		// Arrange
		let features = vec![
			"db-postgres".into(),
			"auth-jwt".into(),
			"websockets".into(),
			"tasks".into(),
			"redis-backend".into(),
			"storage".into(),
			"sessions".into(),
			"grpc".into(),
		];

		// Act
		let signals = InfraSignals::from_features(&features);

		// Assert
		assert_eq!(signals.database, Some("postgresql".to_owned()));
		assert!(signals.jwt);
		assert!(signals.websocket);
		assert!(signals.background_worker);
		assert_eq!(signals.cache, Some("redis".to_owned()));
		assert!(signals.object_storage);
		assert!(signals.sessions);
		assert!(signals.grpc);
	}

	#[rstest]
	fn test_infer_signals_mysql_database() {
		// Arrange
		let features = vec!["db-mysql".into()];

		// Act
		let signals = InfraSignals::from_features(&features);

		// Assert
		assert_eq!(signals.database, Some("mysql".to_owned()));
	}

	#[rstest]
	fn test_infer_signals_duplicate_features() {
		// Arrange
		let features = vec![
			"db-postgres".into(),
			"db-postgres".into(),
			"auth-jwt".into(),
		];

		// Act
		let signals = InfraSignals::from_features(&features);

		// Assert
		assert_eq!(signals.database, Some("postgresql".to_owned()));
		assert!(signals.jwt);
	}

	#[rstest]
	fn test_detect_project_no_features() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			r#"
[package]
name = "bare-app"
version = "0.1.0"

[dependencies]
reinhardt-web = "0.1"
"#,
		)
		.unwrap();

		// Act
		let result = detect_project(dir.path());

		// Assert
		// String dependency has no features table, so extract_features_from_dep returns None
		// and the dependency is not detected via the table-based path
		assert!(result.is_err());
	}
}
