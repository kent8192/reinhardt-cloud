//! Reads the Rust toolchain version from `rust-toolchain.toml`.

use std::path::Path;

/// Read the Rust toolchain channel from `rust-toolchain.toml` in the given directory.
///
/// Returns the `[toolchain].channel` value (e.g., `"1.94.1"`, `"nightly-2025-01-15"`).
pub(crate) fn read_rust_version(project_dir: &Path) -> Result<String, String> {
	let toolchain_path = project_dir.join("rust-toolchain.toml");
	let content = std::fs::read_to_string(&toolchain_path)
		.map_err(|_| format!("rust-toolchain.toml not found in {}", project_dir.display()))?;
	parse_channel(&content)
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
