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
	pub(crate) pages: bool,
	pub(crate) graphql: bool,
	pub(crate) tracing: bool,
	/// Whether the build needs the `protoc` compiler installed in the build
	/// stages of the generated Dockerfile.
	///
	/// Distinct from `grpc` (which is derived from reinhardt-web feature
	/// flags): this signal is detected from `Cargo.lock` so that any
	/// transitive `prost`/`tonic` dependency — including indirect ones
	/// pulled in by reinhardt-cloud-grpc or reinhardt-cloud-proto — also
	/// triggers protoc installation.
	pub(crate) protoc_needed: bool,
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
			pages: has("pages"),
			graphql: has("graphql"),
			tracing: has("telemetry-tracing"),
			// Detected from Cargo.lock by the Dockerfile generator, not from
			// reinhardt-web feature flags. Default to false here so feature-only
			// inference paths (e.g., zero-config introspection) keep behaving
			// the same.
			protoc_needed: false,
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
	validate_package_name(&name)?;

	let version = parsed
		.get("package")
		.and_then(|p| p.get("version"))
		.and_then(|v| v.as_str())
		.unwrap_or("0.1.0")
		.to_owned();

	// Find reinhardt-web dependency (handles package rename pattern,
	// target-cfg sections, and `workspace = true` inheritance).
	let features = find_reinhardt_features(project_dir, &parsed)
		.ok_or("Not a reinhardt-web project: no reinhardt-web dependency found")?;

	let signals = InfraSignals::from_features(&features);

	Ok(ProjectMetadata {
		name,
		version,
		features,
		signals,
	})
}

fn validate_package_name(name: &str) -> Result<(), String> {
	if name.is_empty()
		|| !name
			.bytes()
			.all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
	{
		return Err(format!(
			"Invalid [package].name `{name}` in Cargo.toml: package names may only contain ASCII letters, digits, hyphens, and underscores"
		));
	}

	Ok(())
}

/// Search for reinhardt-web dependency features across every dependency
/// section that Cargo recognizes:
///
/// 1. `[dependencies]`
/// 2. `[target.'<cfg>'.dependencies]` for every cfg expression
/// 3. `[workspace.dependencies]` (when this Cargo.toml IS the workspace root)
///
/// When the matching entry is `{ workspace = true }`, the lookup walks up
/// to the workspace root's `[workspace.dependencies]` and re-runs the same
/// resolver against that table.
fn find_reinhardt_features(project_dir: &Path, cargo_toml: &toml::Value) -> Option<Vec<String>> {
	if let Some(features) = cargo_toml
		.get("dependencies")
		.and_then(|deps| resolve_reinhardt_in_table(project_dir, deps))
	{
		return Some(features);
	}

	// Iterate every `[target.'<cfg>'.dependencies]` sub-table.
	if let Some(target_table) = cargo_toml.get("target").and_then(|t| t.as_table()) {
		for target_section in target_table.values() {
			if let Some(deps) = target_section.get("dependencies")
				&& let Some(features) = resolve_reinhardt_in_table(project_dir, deps)
			{
				return Some(features);
			}
		}
	}

	cargo_toml
		.get("workspace")
		.and_then(|w| w.get("dependencies"))
		.and_then(|deps| resolve_reinhardt_in_table(project_dir, deps))
}

/// Locate the reinhardt-web dependency in a dependency table and extract
/// its features, transparently following `workspace = true` to the
/// workspace root's `[workspace.dependencies]` table.
///
/// Two passes are required because in a workspace member the dependency
/// is declared as `reinhardt = { workspace = true }` — the local table
/// has neither the `reinhardt-web` key nor a `package = "reinhardt-web"`
/// rename, so identification can only happen at the workspace root.
fn resolve_reinhardt_in_table(project_dir: &Path, deps: &toml::Value) -> Option<Vec<String>> {
	// Pass 1: directly identifiable reinhardt-web entry (key match or
	// `package = "reinhardt-web"` rename).
	if let Some((key, dep)) = find_reinhardt_dep_in_table(deps) {
		if dep.get("workspace").and_then(|v| v.as_bool()) == Some(true) {
			return resolve_workspace_features_for_key(project_dir, &key);
		}
		return extract_features_from_dep(dep);
	}

	// Pass 2: any `workspace = true` entry whose key, when resolved against
	// the workspace root, turns out to be reinhardt-web.
	let table = deps.as_table()?;
	for (key, dep) in table {
		if dep.get("workspace").and_then(|v| v.as_bool()) == Some(true)
			&& let Some(features) = resolve_workspace_features_for_key(project_dir, key)
		{
			return Some(features);
		}
	}
	None
}

/// Walk up from `project_dir` to the workspace root, look up `key` in
/// `[workspace.dependencies]`, and — if that entry actually IS reinhardt-web
/// (key match or `package = "reinhardt-web"` rename) — return its features.
fn resolve_workspace_features_for_key(project_dir: &Path, key: &str) -> Option<Vec<String>> {
	let workspace_root = find_workspace_root(project_dir)?;
	let content = std::fs::read_to_string(&workspace_root).ok()?;
	let parsed: toml::Value = toml::from_str(&content).ok()?;
	let deps = parsed
		.get("workspace")
		.and_then(|w| w.get("dependencies"))?;
	let dep = deps.get(key)?;

	let is_reinhardt = key == "reinhardt-web"
		|| dep.get("package").and_then(|p| p.as_str()) == Some("reinhardt-web");
	if !is_reinhardt {
		return None;
	}

	extract_features_from_dep(dep)
}

/// Walk up from `start_dir` (exclusive — the caller's own Cargo.toml is
/// already excluded by the surrounding flow) looking for a `Cargo.toml`
/// containing `[workspace]`. Returns the absolute path to that file.
fn find_workspace_root(start_dir: &Path) -> Option<std::path::PathBuf> {
	let mut current = start_dir.canonicalize().ok()?;
	loop {
		current = current.parent()?.to_path_buf();
		let candidate = current.join("Cargo.toml");
		if candidate.exists()
			&& let Ok(content) = std::fs::read_to_string(&candidate)
			&& let Ok(parsed) = toml::from_str::<toml::Value>(&content)
			&& parsed.get("workspace").is_some()
		{
			return Some(candidate);
		}
	}
}

/// Locate the reinhardt-web dependency entry in a deps table, handling the
/// `package = "reinhardt-web"` rename pattern. Returns `(key, value)` so
/// the caller can either extract features directly or — when `value` is
/// `{ workspace = true }` — re-resolve against the workspace root using
/// the same `key`.
fn find_reinhardt_dep_in_table(deps: &toml::Value) -> Option<(String, &toml::Value)> {
	if let Some(dep) = deps.get("reinhardt-web") {
		return Some(("reinhardt-web".to_owned(), dep));
	}
	let table = deps.as_table()?;
	for (key, dep) in table {
		if dep.get("package").and_then(|p| p.as_str()) == Some("reinhardt-web") {
			return Some((key.clone(), dep));
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
	fn test_detect_project_rejects_shell_metacharacters_in_package_name() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			r#"
[package]
name = "evil; touch /tmp/injected #"
version = "0.2.0"

[dependencies]
reinhardt-web = { version = "0.1", features = ["standard"] }
"#,
		)
		.unwrap();

		// Act
		let result = detect_project(dir.path());

		// Assert
		let error = result.expect_err("unsafe package names must be rejected");
		assert_eq!(
			error,
			"Invalid [package].name `evil; touch /tmp/injected #` in Cargo.toml: package names may only contain ASCII letters, digits, hyphens, and underscores"
		);
	}

	#[rstest]
	#[case("safe-name")]
	#[case("safe_name")]
	#[case("safe123")]
	fn test_package_name_validation_accepts_cargo_safe_names(#[case] name: &str) {
		// Arrange

		// Act
		let result = validate_package_name(name);

		// Assert
		assert_eq!(result, Ok(()));
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

	#[rstest]
	fn test_pages_signal_detected() {
		// Arrange
		let features = vec!["pages".to_string()];

		// Act
		let signals = InfraSignals::from_features(&features);

		// Assert
		assert!(signals.pages);
	}

	#[rstest]
	fn test_graphql_signal_detected() {
		// Arrange
		let features = vec!["graphql".to_string()];

		// Act
		let signals = InfraSignals::from_features(&features);

		// Assert
		assert!(signals.graphql);
	}

	/// `[target.'cfg(...)'.dependencies]` is the standard way to keep
	/// `reinhardt-web` server-side while letting the same crate also build
	/// for `wasm32`. The detector must walk these tables — otherwise every
	/// app crate using the recommended layout (including the dashboard)
	/// fails detection.
	#[rstest]
	fn test_detect_project_target_cfg_dependency() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::write(
			dir.path().join("Cargo.toml"),
			r#"
[package]
name = "target-cfg-app"
version = "0.4.0"

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
reinhardt = { package = "reinhardt-web", version = "0.1", features = ["db-postgres", "auth-jwt"] }
"#,
		)
		.unwrap();

		// Act
		let metadata = detect_project(dir.path()).expect("target-cfg dep must be detected");

		// Assert
		assert_eq!(metadata.name, "target-cfg-app");
		assert!(metadata.features.contains(&"db-postgres".to_owned()));
		assert!(metadata.features.contains(&"auth-jwt".to_owned()));
		assert_eq!(metadata.signals.database, Some("postgresql".to_owned()));
		assert!(metadata.signals.jwt);
	}

	/// `reinhardt = { workspace = true }` in a member crate must follow
	/// through to the workspace root's `[workspace.dependencies]` entry,
	/// where the actual `package = "reinhardt-web"` and feature list live.
	#[rstest]
	fn test_detect_project_workspace_inherited() {
		// Arrange — workspace root
		let workspace_dir = tempfile::tempdir().unwrap();
		std::fs::write(
			workspace_dir.path().join("Cargo.toml"),
			r#"
[workspace]
members = ["app"]

[workspace.dependencies]
reinhardt = { package = "reinhardt-web", version = "0.1", features = ["db-postgres", "websockets"] }
"#,
		)
		.unwrap();
		// Member crate
		let member_dir = workspace_dir.path().join("app");
		std::fs::create_dir(&member_dir).unwrap();
		std::fs::write(
			member_dir.join("Cargo.toml"),
			r#"
[package]
name = "ws-member"
version = "0.5.0"

[dependencies]
reinhardt = { workspace = true }
"#,
		)
		.unwrap();

		// Act
		let metadata =
			detect_project(&member_dir).expect("workspace=true must resolve to root features");

		// Assert
		assert_eq!(metadata.name, "ws-member");
		assert!(metadata.features.contains(&"db-postgres".to_owned()));
		assert!(metadata.features.contains(&"websockets".to_owned()));
		assert_eq!(metadata.signals.database, Some("postgresql".to_owned()));
		assert!(metadata.signals.websocket);
	}

	/// The full dashboard pattern: `workspace = true` declared inside
	/// `[target.'cfg(...)'.dependencies]`. Both gaps must be closed
	/// simultaneously for the detector to succeed.
	#[rstest]
	fn test_detect_project_workspace_inherited_with_target_cfg() {
		// Arrange — workspace root carries the actual rename + features
		let workspace_dir = tempfile::tempdir().unwrap();
		std::fs::write(
			workspace_dir.path().join("Cargo.toml"),
			r#"
[workspace]
members = ["dashboard"]

[workspace.dependencies]
reinhardt = { package = "reinhardt-web", version = "0.1", features = ["db-postgres", "auth-jwt", "sessions"] }
"#,
		)
		.unwrap();
		// Member crate mirrors dashboard/Cargo.toml shape
		let member_dir = workspace_dir.path().join("dashboard");
		std::fs::create_dir(&member_dir).unwrap();
		std::fs::write(
			member_dir.join("Cargo.toml"),
			r#"
[package]
name = "reinhardt-cloud-dashboard"
version = "0.1.0"

[dependencies]
serde = "1"

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
reinhardt = { workspace = true }
"#,
		)
		.unwrap();

		// Act
		let metadata = detect_project(&member_dir)
			.expect("dashboard pattern (target-cfg + workspace=true) must be detected");

		// Assert
		assert_eq!(metadata.name, "reinhardt-cloud-dashboard");
		assert!(metadata.features.contains(&"db-postgres".to_owned()));
		assert!(metadata.features.contains(&"auth-jwt".to_owned()));
		assert!(metadata.features.contains(&"sessions".to_owned()));
		assert_eq!(metadata.signals.database, Some("postgresql".to_owned()));
		assert!(metadata.signals.jwt);
		assert!(metadata.signals.sessions);
	}

	/// The workspace-inheritance path must still honor the
	/// `package = "reinhardt-web"` rename when the workspace key itself is
	/// not literally "reinhardt-web".
	#[rstest]
	fn test_detect_project_workspace_inherited_with_package_rename_in_member() {
		// Arrange — workspace root keys the dep as `reinhardt-web` directly
		let workspace_dir = tempfile::tempdir().unwrap();
		std::fs::write(
			workspace_dir.path().join("Cargo.toml"),
			r#"
[workspace]
members = ["app"]

[workspace.dependencies]
reinhardt-web = { version = "0.1", features = ["db-mysql", "tasks"] }
"#,
		)
		.unwrap();
		let member_dir = workspace_dir.path().join("app");
		std::fs::create_dir(&member_dir).unwrap();
		std::fs::write(
			member_dir.join("Cargo.toml"),
			r#"
[package]
name = "renamed-member"
version = "0.1.0"

[dependencies]
reinhardt-web = { workspace = true }
"#,
		)
		.unwrap();

		// Act
		let metadata = detect_project(&member_dir).unwrap();

		// Assert
		assert_eq!(metadata.signals.database, Some("mysql".to_owned()));
		assert!(metadata.signals.background_worker);
	}

	#[rstest]
	fn test_pages_and_graphql_default_false() {
		// Arrange
		let features: Vec<String> = vec![];

		// Act
		let signals = InfraSignals::from_features(&features);

		// Assert
		assert!(!signals.pages);
		assert!(!signals.graphql);
	}
}
