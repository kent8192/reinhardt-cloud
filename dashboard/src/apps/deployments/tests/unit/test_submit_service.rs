//! Unit tests for the shared deployment submission service.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use crate::apps::clusters::models::Cluster;
	use crate::apps::deployments::services::{
		SubmitProjectDeploymentError, SubmitProjectDeploymentInput, validate_submission_manifest,
	};

	fn active_cluster() -> Cluster {
		Cluster::build()
			.organization(1)
			.name("prod".to_string())
			.api_url("https://cluster.example.test".to_string())
			.is_active(true)
			.token_hash(None)
			.token_last_rotated_at(None)
			.finish()
	}

	fn project_manifest(name: &str, namespace: &str) -> String {
		format!(
			r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: Project
metadata:
  name: {name}
  namespace: {namespace}
spec:
  image: ghcr.io/example/{name}:latest
"#
		)
	}

	#[rstest]
	fn test_validate_submission_manifest_accepts_matching_project() {
		// Arrange
		let cluster = active_cluster();
		let manifest = project_manifest("web", "default");
		let input = SubmitProjectDeploymentInput {
			organization_id: 1,
			project_name: "web",
			cluster: &cluster,
			namespace: Some("default"),
			image: "ghcr.io/example/web:latest",
			project_yaml: &manifest,
		};

		// Act
		let project =
			validate_submission_manifest(input).expect("matching manifest should validate");

		// Assert
		assert_eq!(project.metadata.name.as_deref(), Some("web"));
		assert_eq!(project.metadata.namespace.as_deref(), Some("default"));
		assert_eq!(project.spec.image, "ghcr.io/example/web:latest");
	}

	#[rstest]
	fn test_validate_submission_manifest_rejects_name_mismatch() {
		// Arrange
		let cluster = active_cluster();
		let manifest = project_manifest("api", "default");
		let input = SubmitProjectDeploymentInput {
			organization_id: 1,
			project_name: "web",
			cluster: &cluster,
			namespace: Some("default"),
			image: "ghcr.io/example/web:latest",
			project_yaml: &manifest,
		};

		// Act
		let error = validate_submission_manifest(input).unwrap_err();

		// Assert
		assert_eq!(
			error,
			SubmitProjectDeploymentError::BadRequest(
				"Project YAML metadata.name must match project_name".to_string()
			)
		);
		assert_eq!(error.status_code(), 400);
	}

	#[rstest]
	fn test_validate_submission_manifest_rejects_namespace_mismatch() {
		// Arrange
		let cluster = active_cluster();
		let manifest = project_manifest("web", "production");
		let input = SubmitProjectDeploymentInput {
			organization_id: 1,
			project_name: "web",
			cluster: &cluster,
			namespace: Some("default"),
			image: "ghcr.io/example/web:latest",
			project_yaml: &manifest,
		};

		// Act
		let error = validate_submission_manifest(input).unwrap_err();

		// Assert
		assert_eq!(
			error,
			SubmitProjectDeploymentError::BadRequest(
				"Project YAML metadata.namespace must match namespace".to_string()
			)
		);
		assert_eq!(error.status_code(), 400);
	}

	#[rstest]
	fn test_validate_submission_manifest_rejects_image_mismatch() {
		// Arrange
		let cluster = active_cluster();
		let manifest = project_manifest("web", "default");
		let input = SubmitProjectDeploymentInput {
			organization_id: 1,
			project_name: "web",
			cluster: &cluster,
			namespace: Some("default"),
			image: "ghcr.io/example/web:other",
			project_yaml: &manifest,
		};

		// Act
		let error = validate_submission_manifest(input).unwrap_err();

		// Assert
		assert_eq!(
			error,
			SubmitProjectDeploymentError::BadRequest(
				"Project YAML spec.image must match image".to_string()
			)
		);
		assert_eq!(error.status_code(), 400);
	}

	#[rstest]
	fn test_validate_submission_manifest_rejects_missing_yaml() {
		// Arrange
		let cluster = active_cluster();
		let input = SubmitProjectDeploymentInput {
			organization_id: 1,
			project_name: "web",
			cluster: &cluster,
			namespace: Some("default"),
			image: "ghcr.io/example/web:latest",
			project_yaml: "   ",
		};

		// Act
		let error = validate_submission_manifest(input).unwrap_err();

		// Assert
		assert_eq!(
			error,
			SubmitProjectDeploymentError::BadRequest("Project YAML is required".to_string())
		);
		assert_eq!(error.status_code(), 400);
	}
}
