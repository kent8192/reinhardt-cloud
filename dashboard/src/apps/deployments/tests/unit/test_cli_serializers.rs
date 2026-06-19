//! Unit tests for CLI deployment serializers.

#[cfg(test)]
mod tests {
	use reinhardt::Validate;
	use rstest::rstest;
	use serde_json::json;

	use crate::apps::deployments::serializers::{CliDeploymentRequest, CliDeploymentResponse};

	fn valid_request() -> CliDeploymentRequest {
		CliDeploymentRequest {
			project_name: "demo".to_string(),
			cluster: "prod".to_string(),
			namespace: "default".to_string(),
			image: "ghcr.io/example/demo:latest".to_string(),
			project_yaml: "apiVersion: paas.reinhardt-cloud.dev/v1alpha2\nkind: Project\n"
				.to_string(),
		}
	}

	#[rstest]
	fn test_cli_deployment_request_deserializes_expected_fields() {
		// Arrange
		let body = json!({
			"project_name": "demo",
			"cluster": "prod",
			"namespace": "default",
			"image": "ghcr.io/example/demo:latest",
			"project_yaml": "apiVersion: paas.reinhardt-cloud.dev/v1alpha2\nkind: Project\n",
		});

		// Act
		let request: CliDeploymentRequest = serde_json::from_value(body).unwrap();

		// Assert
		assert_eq!(request.project_name, "demo");
		assert_eq!(request.cluster, "prod");
		assert_eq!(request.namespace, "default");
		assert_eq!(request.image, "ghcr.io/example/demo:latest");
		assert_eq!(
			request.project_yaml,
			"apiVersion: paas.reinhardt-cloud.dev/v1alpha2\nkind: Project\n"
		);
		assert!(request.validate().is_ok());
	}

	#[rstest]
	#[case::project_name_min("project_name", "a", true)]
	#[case::project_name_empty("project_name", "", false)]
	#[case::project_name_max("project_name", &"a".repeat(63), true)]
	#[case::project_name_too_long("project_name", &"a".repeat(64), false)]
	#[case::cluster_min("cluster", "p", true)]
	#[case::cluster_empty("cluster", "", false)]
	#[case::cluster_max("cluster", &"p".repeat(63), true)]
	#[case::cluster_too_long("cluster", &"p".repeat(64), false)]
	#[case::namespace_min("namespace", "d", true)]
	#[case::namespace_empty("namespace", "", false)]
	#[case::namespace_max("namespace", &"d".repeat(63), true)]
	#[case::namespace_too_long("namespace", &"d".repeat(64), false)]
	#[case::image_min("image", "n", true)]
	#[case::image_empty("image", "", false)]
	#[case::image_max("image", &"n".repeat(512), true)]
	#[case::image_too_long("image", &"n".repeat(513), false)]
	#[case::project_yaml_min("project_yaml", "a", true)]
	#[case::project_yaml_empty("project_yaml", "", false)]
	#[case::project_yaml_max("project_yaml", &"a".repeat(65535), true)]
	#[case::project_yaml_too_long("project_yaml", &"a".repeat(65536), false)]
	fn test_cli_deployment_request_validation_boundaries(
		#[case] field: &str,
		#[case] value: &str,
		#[case] expected_valid: bool,
	) {
		// Arrange
		let mut request = valid_request();
		match field {
			"project_name" => request.project_name = value.to_string(),
			"cluster" => request.cluster = value.to_string(),
			"namespace" => request.namespace = value.to_string(),
			"image" => request.image = value.to_string(),
			"project_yaml" => request.project_yaml = value.to_string(),
			_ => unreachable!("unknown test field"),
		}

		// Act
		let result = request.validate();

		// Assert
		assert_eq!(result.is_ok(), expected_valid);
	}

	#[rstest]
	fn test_cli_deployment_response_json_roundtrips() {
		// Arrange
		let response = CliDeploymentResponse {
			deployment_id: 7,
			project_name: "demo".to_string(),
			cluster: "prod".to_string(),
			status: "pending".to_string(),
			image: "ghcr.io/example/demo:latest".to_string(),
		};

		// Act
		let json = serde_json::to_value(&response).unwrap();
		let roundtrip: CliDeploymentResponse = serde_json::from_value(json.clone()).unwrap();

		// Assert
		assert_eq!(
			json,
			json!({
				"deployment_id": 7,
				"project_name": "demo",
				"cluster": "prod",
				"status": "pending",
				"image": "ghcr.io/example/demo:latest",
			})
		);
		assert_eq!(roundtrip, response);
	}
}
