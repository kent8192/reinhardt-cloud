//! Tests for deployments app serializers.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use serde_json;

	use crate::apps::deployments::serializers::CreateDeploymentRequest;

	#[rstest]
	fn test_create_deployment_request_deserializes() {
		// Arrange
		let json = r#"{"app_name": "web", "cluster_id": 42, "image": "nginx:latest"}"#;

		// Act
		let req: CreateDeploymentRequest = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(req.app_name, "web");
		assert_eq!(req.cluster_id, 42);
		assert_eq!(req.image, "nginx:latest");
		assert!(req.reinhardt_app_yaml.is_none());
	}

	#[rstest]
	fn test_create_deployment_request_accepts_reinhardt_app_yaml() {
		// Arrange
		let json = r#"{
			"app_name": "web",
			"cluster_id": 42,
			"image": "nginx:latest",
			"reinhardt_app_yaml": "apiVersion: paas.reinhardt-cloud.dev/v1alpha2\nkind: ReinhardtApp\n"
		}"#;

		// Act
		let req: CreateDeploymentRequest = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(
			req.reinhardt_app_yaml.as_deref(),
			Some("apiVersion: paas.reinhardt-cloud.dev/v1alpha2\nkind: ReinhardtApp\n")
		);
	}
}
