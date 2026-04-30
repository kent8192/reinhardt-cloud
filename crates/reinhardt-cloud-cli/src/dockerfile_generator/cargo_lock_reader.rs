//! Extracts dependency versions from `Cargo.lock` content.

/// Extracts the `wasm-bindgen` version from parsed `Cargo.lock` content.
///
/// Looks for `[[package]]` entries with an exact name match of `"wasm-bindgen"`,
/// ignoring related crates like `wasm-bindgen-macro` or `wasm-bindgen-shared`.
///
/// Returns `Ok(Some(version))` for the first matching entry, `Ok(None)` if no
/// entry is found or the content is empty, and `Err` if the TOML is malformed.
pub(super) fn extract_wasm_bindgen_version(content: &str) -> Result<Option<String>, String> {
	if content.trim().is_empty() {
		return Ok(None);
	}

	let parsed: toml::Value =
		toml::from_str(content).map_err(|e| format!("failed to parse Cargo.lock: {e}"))?;

	let packages = match parsed.get("package").and_then(|v| v.as_array()) {
		Some(pkgs) => pkgs,
		None => return Ok(None),
	};

	for pkg in packages {
		let name = pkg.get("name").and_then(|v| v.as_str()).unwrap_or("");
		if name == "wasm-bindgen"
			&& let Some(version) = pkg.get("version").and_then(|v| v.as_str())
		{
			return Ok(Some(version.to_owned()));
		}
	}

	Ok(None)
}

/// Returns `true` when `prost`, `prost-build`, `tonic`, or `tonic-build`
/// appears anywhere in the workspace dependency graph captured by
/// `Cargo.lock`.
///
/// The lockfile is the authoritative source for this check because it
/// reflects the fully-resolved dependency tree, including transitive
/// dependencies pulled in by build scripts (e.g., `tonic-build` is a
/// `[build-dependencies]` entry of `reinhardt-cloud-grpc`). Walking only
/// the project's `Cargo.toml` would miss those.
///
/// Returns `false` when the content is empty, the TOML is malformed, or
/// no matching package is found. The function is intentionally lenient
/// — a missing or unreadable lockfile must not block Dockerfile
/// generation.
pub(super) fn detect_protoc_requirement(cargo_lock_content: &str) -> bool {
	if cargo_lock_content.trim().is_empty() {
		return false;
	}

	let parsed: toml::Value = match toml::from_str(cargo_lock_content) {
		Ok(value) => value,
		Err(_) => return false,
	};

	let packages = match parsed.get("package").and_then(|v| v.as_array()) {
		Some(pkgs) => pkgs,
		None => return false,
	};

	for pkg in packages {
		let name = pkg.get("name").and_then(|v| v.as_str()).unwrap_or("");
		if matches!(name, "prost" | "prost-build" | "tonic" | "tonic-build") {
			return true;
		}
	}

	false
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::*;

	// C1: Standard Cargo.lock with wasm-bindgen 0.2.100
	#[rstest]
	fn extract_wasm_bindgen_version_found() {
		// Arrange
		let content = r#"
[[package]]
name = "serde"
version = "1.0.200"

[[package]]
name = "wasm-bindgen"
version = "0.2.100"
source = "registry+https://github.com/rust-lang/crates.io-index"
"#;

		// Act
		let result = extract_wasm_bindgen_version(content);

		// Assert
		assert_eq!(result, Ok(Some("0.2.100".to_owned())));
	}

	// C2: Two wasm-bindgen entries — first one wins
	#[rstest]
	fn multiple_versions_takes_first() {
		// Arrange
		let content = r#"
[[package]]
name = "wasm-bindgen"
version = "0.2.99"

[[package]]
name = "wasm-bindgen"
version = "0.2.100"
"#;

		// Act
		let result = extract_wasm_bindgen_version(content);

		// Assert
		assert_eq!(result, Ok(Some("0.2.99".to_owned())));
	}

	// C3: Only serde present — no wasm-bindgen
	#[rstest]
	fn no_wasm_bindgen_entry() {
		// Arrange
		let content = r#"
[[package]]
name = "serde"
version = "1.0.200"
"#;

		// Act
		let result = extract_wasm_bindgen_version(content);

		// Assert
		assert_eq!(result, Ok(None));
	}

	// C4: Empty string input
	#[rstest]
	fn empty_cargo_lock() {
		// Act
		let result = extract_wasm_bindgen_version("");

		// Assert
		assert_eq!(result, Ok(None));
	}

	// C5: Invalid TOML content
	#[rstest]
	fn malformed_cargo_lock() {
		// Arrange
		let content = "this is not valid [[[ toml";

		// Act
		let result = extract_wasm_bindgen_version(content);

		// Assert
		assert!(result.is_err());
	}

	// C6: Only wasm-bindgen-macro and wasm-bindgen-shared — exact name match required
	#[rstest]
	fn wasm_bindgen_macro_ignored() {
		// Arrange
		let content = r#"
[[package]]
name = "wasm-bindgen-macro"
version = "0.2.100"

[[package]]
name = "wasm-bindgen-shared"
version = "0.2.100"
"#;

		// Act
		let result = extract_wasm_bindgen_version(content);

		// Assert
		assert_eq!(result, Ok(None));
	}

	// C7: Pre-release version string
	#[rstest]
	fn prerelease_version() {
		// Arrange
		let content = r#"
[[package]]
name = "wasm-bindgen"
version = "0.3.0-alpha.1"
"#;

		// Act
		let result = extract_wasm_bindgen_version(content);

		// Assert
		assert_eq!(result, Ok(Some("0.3.0-alpha.1".to_owned())));
	}
}
