//! Tests for deployments app serializers.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use serde_json;

	use crate::apps::deployments::models::Deployment;
	use crate::apps::deployments::serializers::{CreateDeploymentRequest, DeploymentResponse};

	#[rstest]
	fn test_deployment_response_status_serializes_to_string() {
		// Arrange
		let deployment = Deployment::build()
			.organization_id(1)
			.app_name("my-app".to_string())
			.cluster_id(1)
			.status("pending".to_string())
			.image("ghcr.io/my-app:latest".to_string())
			.finish();

		// Act
		let response = DeploymentResponse::from(deployment);

		// Assert
		assert_eq!(response.status, "pending");
		assert_eq!(response.app_name, "my-app");
		assert_eq!(response.image, "ghcr.io/my-app:latest");
	}

	#[rstest]
	fn test_deployment_response_with_none_id_serializes_to_null() {
		// Arrange
		let deployment = Deployment::build()
			.organization_id(1)
			.app_name("my-app".to_string())
			.cluster_id(1)
			.status("pending".to_string())
			.image("ghcr.io/my-app:latest".to_string())
			.finish();

		// Act
		let response = DeploymentResponse::from(deployment);
		let json = serde_json::to_value(&response).unwrap();

		// Assert
		assert_eq!(response.id, None);
		assert!(json["id"].is_null());
	}

	#[rstest]
	fn test_deployment_response_with_some_id_serializes_to_number() {
		// Arrange
		let mut deployment = Deployment::build()
			.organization_id(1)
			.app_name("my-app".to_string())
			.cluster_id(1)
			.status("running".to_string())
			.image("ghcr.io/my-app:v2".to_string())
			.finish();
		deployment.id = Some(42);

		// Act
		let response = DeploymentResponse::from(deployment);
		let json = serde_json::to_value(&response).unwrap();

		// Assert
		assert_eq!(response.id, Some(42));
		assert_eq!(json["id"], 42);
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
