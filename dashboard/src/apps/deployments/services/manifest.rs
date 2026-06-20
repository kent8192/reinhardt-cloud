//! `Project` manifest validation helpers.

use reinhardt_cloud_types::crd::Project;

pub fn validate_project_manifest(manifest: &str) -> Result<Option<Project>, String> {
	if manifest.trim().is_empty() {
		return Ok(None);
	}
	let project: Project =
		serde_yaml::from_str(manifest).map_err(|e| format!("Invalid Project YAML: {e}"))?;
	if let Err(errors) = project.spec.validate() {
		let messages = errors
			.into_iter()
			.map(|e| e.message)
			.collect::<Vec<_>>()
			.join("; ");
		return Err(format!("Invalid Project spec: {messages}"));
	}
	Ok(Some(project))
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use super::validate_project_manifest;

	const VALID_MANIFEST: &str = r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: Project
metadata:
  name: web
  namespace: default
spec:
  image: ghcr.io/example/web:latest
"#;

	#[rstest]
	fn test_validate_project_manifest_accepts_empty_form_value() {
		// Arrange / Act
		let result = validate_project_manifest("   ").expect("empty manifest should be accepted");

		// Assert
		assert!(result.is_none());
	}

	#[rstest]
	fn test_validate_project_manifest_accepts_valid_project() {
		// Act
		let result = validate_project_manifest(VALID_MANIFEST)
			.expect("valid manifest should parse")
			.expect("non-empty manifest should return a project");

		// Assert
		assert_eq!(result.metadata.name.as_deref(), Some("web"));
		assert_eq!(result.metadata.namespace.as_deref(), Some("default"));
		assert_eq!(result.spec.image, "ghcr.io/example/web:latest");
	}

	#[rstest]
	fn test_validate_project_manifest_reports_yaml_prefix() {
		// Arrange
		let manifest = "apiVersion: [";

		// Act
		let error = validate_project_manifest(manifest).unwrap_err();

		// Assert — parser detail text can vary across serde_yaml versions.
		assert!(error.starts_with("Invalid Project YAML: "));
	}

	#[rstest]
	fn test_validate_project_manifest_reports_invalid_spec() {
		// Arrange
		let manifest = r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: Project
metadata:
  name: web
  namespace: default
spec:
  image: ghcr.io/example/web:latest
  replicas: -1
"#;

		// Act
		let error = validate_project_manifest(manifest).unwrap_err();

		// Assert
		assert_eq!(error, "Invalid Project spec: spec.replicas must be >= 0");
	}
}
