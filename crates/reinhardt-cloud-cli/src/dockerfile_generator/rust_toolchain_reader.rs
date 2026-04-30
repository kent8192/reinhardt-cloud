//! Reads the Rust toolchain version from `rust-toolchain.toml`.

use std::path::{Path, PathBuf};

/// Read the Rust toolchain channel from `rust-toolchain.toml`.
///
/// Searches the given directory first, then walks up parent directories
/// looking for the file — `rust-toolchain.toml` is conventionally placed at
/// the workspace root in Cargo workspaces, so a member crate's directory
/// will not contain its own copy.
///
/// Returns the `[toolchain].channel` value (e.g., `"1.94.1"`, `"nightly-2025-01-15"`).
pub(crate) fn read_rust_version(project_dir: &Path) -> Result<String, String> {
	let toolchain_path = locate_toolchain_file(project_dir).ok_or_else(|| {
		format!(
			"rust-toolchain.toml not found in {} or any parent directory",
			project_dir.display()
		)
	})?;
	let content = std::fs::read_to_string(&toolchain_path)
		.map_err(|e| format!("Failed to read {}: {e}", toolchain_path.display()))?;
	parse_channel(&content)
}

/// Walk from `start_dir` upward looking for `rust-toolchain.toml`. Returns
/// the first match (closest to the start directory).
fn locate_toolchain_file(start_dir: &Path) -> Option<PathBuf> {
	let mut current = start_dir.canonicalize().ok()?;
	loop {
		let candidate = current.join("rust-toolchain.toml");
		if candidate.exists() {
			return Some(candidate);
		}
		current = current.parent()?.to_path_buf();
	}
}

/// Parse the `[toolchain].channel` value from TOML content.
fn parse_channel(content: &str) -> Result<String, String> {
	let parsed: toml::Value =
		toml::from_str(content).map_err(|e| format!("Failed to parse rust-toolchain.toml: {e}"))?;

	parsed
		.get("toolchain")
		.and_then(|t| t.get("channel"))
		.and_then(|c| c.as_str())
		.map(|s| s.to_owned())
		.ok_or_else(|| "Missing [toolchain].channel in rust-toolchain.toml".to_owned())
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn r1_standard_toolchain() {
		// Arrange
		let content = r#"
[toolchain]
channel = "1.94.1"
"#;

		// Act
		let result = parse_channel(content).unwrap();

		// Assert
		assert_eq!(result, "1.94.1");
	}

	#[rstest]
	fn r2_nightly_channel() {
		// Arrange
		let content = r#"
[toolchain]
channel = "nightly-2025-01-15"
"#;

		// Act
		let result = parse_channel(content).unwrap();

		// Assert
		assert_eq!(result, "nightly-2025-01-15");
	}

	#[rstest]
	fn r3_file_not_found() {
		// Arrange
		let nonexistent = Path::new("/tmp/nonexistent-project-dir-rust-toolchain");

		// Act
		let result = read_rust_version(nonexistent);

		// Assert
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("not found"));
	}

	#[rstest]
	fn r4_missing_channel_field() {
		// Arrange
		let content = r#"
[toolchain]
profile = "minimal"
"#;

		// Act
		let result = parse_channel(content);

		// Assert
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("Missing [toolchain].channel"));
	}

	#[rstest]
	fn r5_empty_file() {
		// Arrange
		let content = "";

		// Act
		let result = parse_channel(content);

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn r6_channel_with_minor_only() {
		// Arrange
		let content = r#"
[toolchain]
channel = "1.94"
"#;

		// Act
		let result = parse_channel(content).unwrap();

		// Assert
		assert_eq!(result, "1.94");
	}

	/// `rust-toolchain.toml` is conventionally placed at the workspace
	/// root, so reading from a member crate must walk up to find it.
	#[rstest]
	fn r8_finds_toolchain_in_workspace_root() {
		// Arrange — workspace root with the toolchain file
		let workspace_dir = tempfile::tempdir().unwrap();
		std::fs::write(
			workspace_dir.path().join("rust-toolchain.toml"),
			r#"
[toolchain]
channel = "1.95.0"
"#,
		)
		.unwrap();
		// Member crate without its own toolchain file
		let member_dir = workspace_dir.path().join("dashboard");
		std::fs::create_dir(&member_dir).unwrap();

		// Act
		let result = read_rust_version(&member_dir).unwrap();

		// Assert
		assert_eq!(result, "1.95.0");
	}

	/// A toolchain file in the member directory takes precedence over one
	/// in a parent directory.
	#[rstest]
	fn r9_member_toolchain_wins_over_parent() {
		// Arrange
		let workspace_dir = tempfile::tempdir().unwrap();
		std::fs::write(
			workspace_dir.path().join("rust-toolchain.toml"),
			r#"
[toolchain]
channel = "1.90.0"
"#,
		)
		.unwrap();
		let member_dir = workspace_dir.path().join("dashboard");
		std::fs::create_dir(&member_dir).unwrap();
		std::fs::write(
			member_dir.join("rust-toolchain.toml"),
			r#"
[toolchain]
channel = "1.95.0"
"#,
		)
		.unwrap();

		// Act
		let result = read_rust_version(&member_dir).unwrap();

		// Assert — closer-to-start match wins
		assert_eq!(result, "1.95.0");
	}

	#[rstest]
	fn r7_stable_channel() {
		// Arrange
		let content = r#"
[toolchain]
channel = "stable"
"#;

		// Act
		let result = parse_channel(content).unwrap();

		// Assert
		assert_eq!(result, "stable");
	}
}
