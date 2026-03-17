//! Tests for deployments app serializers.

#[cfg(test)]
mod tests {
	use rstest::rstest;
	use uuid::Uuid;

	use crate::apps::deployments::models::Deployment;
	use crate::apps::deployments::serializers::{CreateDeploymentRequest, DeploymentResponse};

	#[rstest]
	fn test_deployment_response_status_serializes_to_string() {
		// Arrange
		let deployment = Deployment::new(
			Uuid::new_v4(),
			"my-app".to_string(),
			1,
			"pending".to_string(),
			"ghcr.io/my-app:latest".to_string(),
		);

		// Act
		let response = DeploymentResponse::from(deployment);

		// Assert
		assert_eq!(response.status, "pending");
		assert_eq!(response.app_name, "my-app");
		assert_eq!(response.image, "ghcr.io/my-app:latest");
	}

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
	}
}
