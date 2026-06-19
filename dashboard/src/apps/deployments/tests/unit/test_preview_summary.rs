//! Tests for deployment preview summary helpers.

#[cfg(test)]
mod tests {
	use reinhardt_cloud_types::crd::{PreviewStatus, ProjectPhase};
	use rstest::rstest;

	use crate::apps::deployments::server_fn::{
		PreviewProjectRef, PreviewSummary, preview_project_ref_from_yaml,
		preview_summary_from_status,
	};

	#[rstest]
	fn preview_summary_from_status_preserves_fields_and_serializes_running_phase() {
		// Arrange
		let status = PreviewStatus {
			name: "my-app-pr-42".to_string(),
			pr_number: "42".to_string(),
			url: Some("https://my-app-pr-42.preview.example.com".to_string()),
			phase: Some(ProjectPhase::Running),
			ready_replicas: Some(2),
			last_activity: Some("2026-06-18T12:34:56+09:00".to_string()),
		};

		// Act
		let summary = preview_summary_from_status(status);

		// Assert
		assert_eq!(
			summary,
			PreviewSummary {
				name: "my-app-pr-42".to_string(),
				pr_number: "42".to_string(),
				url: Some("https://my-app-pr-42.preview.example.com".to_string()),
				phase: Some("running".to_string()),
				ready_replicas: Some(2),
				last_activity: Some("2026-06-18T12:34:56+09:00".to_string()),
			}
		);
	}

	#[rstest]
	fn preview_project_ref_from_yaml_uses_manifest_namespace() {
		// Arrange
		let yaml = r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: Project
metadata:
  name: my-app
  namespace: previews
spec:
  image: ghcr.io/example/my-app:latest
"#;

		// Act
		let preview_ref = preview_project_ref_from_yaml("my-app", Some(yaml), "default")
			.expect("YAML should map");

		// Assert
		assert_eq!(
			preview_ref,
			PreviewProjectRef {
				namespace: "previews".to_string(),
				name: "my-app".to_string(),
			}
		);
	}

	#[rstest]
	fn preview_project_ref_from_yaml_falls_back_to_default_namespace() {
		// Arrange
		let yaml = r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: Project
metadata:
  name: my-app
spec:
  image: ghcr.io/example/my-app:latest
"#;

		// Act
		let preview_ref = preview_project_ref_from_yaml("my-app", Some(yaml), "default")
			.expect("YAML should map");

		// Assert
		assert_eq!(
			preview_ref,
			PreviewProjectRef {
				namespace: "default".to_string(),
				name: "my-app".to_string(),
			}
		);
	}

	#[rstest]
	fn preview_project_ref_from_yaml_rejects_missing_yaml() {
		// Arrange
		let project_yaml = None;

		// Act
		let err = preview_project_ref_from_yaml("my-app", project_yaml, "default")
			.expect_err("missing YAML should fail");

		// Assert
		assert_eq!(err, "Project manifest is not available");
	}

	#[rstest]
	fn preview_project_ref_from_yaml_rejects_manifest_name_mismatch() {
		// Arrange
		let yaml = r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: Project
metadata:
  name: other-app
  namespace: previews
spec:
  image: ghcr.io/example/other-app:latest
"#;

		// Act
		let err = preview_project_ref_from_yaml("my-app", Some(yaml), "default")
			.expect_err("name mismatch should fail");

		// Assert
		assert_eq!(
			err,
			"Project manifest points to 'other-app', expected 'my-app'"
		);
	}
}
