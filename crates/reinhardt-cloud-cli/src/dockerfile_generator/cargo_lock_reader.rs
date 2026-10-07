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
			validate_package_version(version)?;
			return Ok(Some(version.to_owned()));
		}
	}

	Ok(None)
}

fn validate_package_version(version: &str) -> Result<(), String> {
	if version.is_empty()
		|| !version
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
	{
		return Err(
			"invalid wasm-bindgen version in Cargo.lock: expected a Dockerfile-safe token"
				.to_owned(),
		);
	}
	Ok(())
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

/// Reject generated Pages images when the resolved framework lacks buildstatic.
pub(super) fn require_buildstatic(
	content: Option<&str>,
	project_name: &str,
	project_version: &str,
) -> Result<(), String> {
	let content = content.ok_or(
		"Pages Dockerfile generation requires Cargo.lock with reinhardt-commands >=0.4.0-alpha.20",
	)?;
	let parsed: toml::Value =
		toml::from_str(content).map_err(|error| format!("failed to parse Cargo.lock: {error}"))?;
	let packages = parsed
		.get("package")
		.and_then(toml::Value::as_array)
		.ok_or("Cargo.lock has no package graph")?;
	let roots: Vec<_> = packages
		.iter()
		.enumerate()
		.filter(|(_, package)| {
			package.get("name").and_then(toml::Value::as_str) == Some(project_name)
				&& package.get("version").and_then(toml::Value::as_str) == Some(project_version)
				&& package.get("source").is_none()
		})
		.map(|(index, _)| index)
		.collect();
	if roots.len() != 1 {
		return Err(format!(
			"Cargo.lock must identify the local application {project_name}@{project_version}; regenerate its lockfile"
		));
	}
	let minimum =
		semver::Version::parse("0.4.0-alpha.20").expect("valid minimum framework version");
	let mut pending = roots;
	let mut visited = std::collections::BTreeSet::new();
	let mut commands_found = false;
	while let Some(index) = pending.pop() {
		if !visited.insert(index) {
			continue;
		}
		let package = &packages[index];
		if package.get("name").and_then(toml::Value::as_str) == Some("reinhardt-commands") {
			let version = package
				.get("version")
				.and_then(toml::Value::as_str)
				.and_then(|version| semver::Version::parse(version).ok());
			if version.is_none_or(|version| version < minimum) {
				return Err("the selected application requires reinhardt-commands >=0.4.0-alpha.20 for buildstatic; update and lock its framework, or provide a custom Dockerfile".into());
			}
			commands_found = true;
		}
		for dependency in package
			.get("dependencies")
			.and_then(toml::Value::as_array)
			.into_iter()
			.flatten()
		{
			let dependency = dependency.as_str().ok_or("invalid Cargo.lock dependency")?;
			let fields: Vec<_> = dependency.split_whitespace().collect();
			let name = fields.first().ok_or("empty Cargo.lock dependency")?;
			let version = fields.get(1).filter(|value| !value.starts_with('('));
			let source = fields
				.iter()
				.find(|value| value.starts_with('('))
				.map(|value| value.trim_start_matches('(').trim_end_matches(')'));
			let matches: Vec<_> = packages
				.iter()
				.enumerate()
				.filter(|(_, candidate)| {
					candidate.get("name").and_then(toml::Value::as_str) == Some(*name)
						&& version.is_none_or(|version| {
							candidate.get("version").and_then(toml::Value::as_str) == Some(*version)
						}) && source.is_none_or(|source| {
						candidate.get("source").and_then(toml::Value::as_str) == Some(source)
					})
				})
				.map(|(index, _)| index)
				.collect();
			if matches.len() != 1 {
				return Err(format!(
					"Cargo.lock dependency {dependency} is missing or ambiguous; regenerate the application's lockfile"
				));
			}
			pending.extend(matches);
		}
	}
	if !commands_found {
		return Err("the selected application's Cargo.lock dependency graph has no reinhardt-commands for buildstatic; update and lock its framework, or provide a custom Dockerfile".into());
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::*;

	#[rstest]
	#[case("0.4.0-alpha.14", false)]
	#[case("0.4.0-alpha.19", false)]
	#[case("0.4.0-alpha.20", true)]
	#[case("0.4.0", true)]
	fn buildstatic_requires_a_capable_resolved_framework(
		#[case] version: &str,
		#[case] available: bool,
	) {
		let content = format!("[[package]]\nname='reinhardt-commands'\nversion='{version}'");
		assert_eq!(
			require_buildstatic(Some(&content), "reinhardt-commands", version).is_ok(),
			available
		);
	}

	#[rstest]
	#[case("0.4.0-alpha.20", true)]
	#[case("0.4.0-alpha.19", false)]
	fn buildstatic_uses_only_the_selected_application_dependency_graph(
		#[case] selected_version: &str,
		#[case] accepted: bool,
	) {
		let other_version = if selected_version.ends_with("20") {
			"0.4.0-alpha.19"
		} else {
			"0.4.0-alpha.20"
		};
		let content = format!(
			r#"
[[package]]
name = "selected-app"
version = "0.1.0"
dependencies = ["framework {selected_version}"]
[[package]]
name = "framework"
version = "{selected_version}"
dependencies = ["reinhardt-commands {selected_version}"]
[[package]]
name = "unrelated-app"
version = "0.1.0"
dependencies = ["reinhardt-commands {other_version}"]
[[package]]
name = "reinhardt-commands"
version = "{selected_version}"
[[package]]
name = "reinhardt-commands"
version = "{other_version}"
"#
		);
		assert_eq!(
			require_buildstatic(Some(&content), "selected-app", "0.1.0").is_ok(),
			accepted
		);
	}

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

	#[rstest]
	fn rejects_wasm_bindgen_version_with_shell_metacharacters() {
		// Arrange
		let content = r#"
[[package]]
name = "wasm-bindgen"
version = "0.2.100; echo injected"
"#;

		// Act
		let result = extract_wasm_bindgen_version(content);

		// Assert
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("Dockerfile-safe token"));
	}

	// P1: prost-build present (build-dep transitively requires protoc)
	#[rstest]
	fn detect_protoc_with_prost_build() {
		// Arrange
		let content = r#"
[[package]]
name = "serde"
version = "1.0.200"

[[package]]
name = "prost-build"
version = "0.13.0"
"#;

		// Act
		let result = detect_protoc_requirement(content);

		// Assert
		assert!(result);
	}

	// P2: tonic-build present (build-dep — direct trigger from #477)
	#[rstest]
	fn detect_protoc_with_tonic_build() {
		// Arrange
		let content = r#"
[[package]]
name = "tonic-build"
version = "0.13.0"
"#;

		// Act
		let result = detect_protoc_requirement(content);

		// Assert
		assert!(result);
	}

	// P3: tonic runtime only — no build-dep, but still requires protoc somewhere
	// in the workspace (typically via reinhardt-cloud-grpc) so detection must
	// still fire.
	#[rstest]
	fn detect_protoc_with_tonic_runtime_only() {
		// Arrange
		let content = r#"
[[package]]
name = "tonic"
version = "0.13.0"
"#;

		// Act
		let result = detect_protoc_requirement(content);

		// Assert
		assert!(result);
	}

	// P4: completely unrelated dependency tree — must NOT fire
	#[rstest]
	fn detect_protoc_with_no_prost_or_tonic() {
		// Arrange
		let content = r#"
[[package]]
name = "serde"
version = "1.0.200"

[[package]]
name = "tokio"
version = "1.40.0"
"#;

		// Act
		let result = detect_protoc_requirement(content);

		// Assert
		assert!(!result);
	}

	// P5: malformed lockfile must not panic and must be lenient (false)
	#[rstest]
	fn detect_protoc_malformed_returns_false() {
		// Act
		let result = detect_protoc_requirement("not [[ valid toml");

		// Assert
		assert!(!result);
	}

	// P6: empty content returns false
	#[rstest]
	fn detect_protoc_empty_returns_false() {
		// Act
		let result = detect_protoc_requirement("");

		// Assert
		assert!(!result);
	}
}
