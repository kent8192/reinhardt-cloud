//! Boundary validation tests for CreateDeploymentRequest.

#[cfg(test)]
mod tests {
	use reinhardt::Validate;
	use rstest::rstest;

	use crate::apps::deployments::serializers::CreateDeploymentRequest;

	/// Validate app_name length boundaries (min=1, max=63).
	#[rstest]
	#[case("a", true)]
	#[case("", false)]
	#[case(&"a".repeat(63), true)]
	#[case(&"a".repeat(64), false)]
	fn test_deployment_app_name_boundary(#[case] app_name: &str, #[case] valid: bool) {
		// Arrange
		let req = CreateDeploymentRequest {
			app_name: app_name.to_string(),
			cluster_id: 1,
			image: "nginx:latest".to_string(),
			reinhardt_app_yaml: None,
		};

		// Act
		let result = req.validate();

		// Assert
		assert_eq!(
			result.is_ok(),
			valid,
			"app_name={app_name:?} expected valid={valid}"
		);
	}

	/// Validate cluster_id range boundaries (min=1).
	#[rstest]
	#[case(1i64, true)]
	#[case(0i64, false)]
	#[case(-1i64, false)]
	#[case(i64::MAX, true)]
	fn test_deployment_cluster_id_boundary(#[case] cluster_id: i64, #[case] valid: bool) {
		// Arrange
		let req = CreateDeploymentRequest {
			app_name: "my-app".to_string(),
			cluster_id,
			image: "nginx:latest".to_string(),
			reinhardt_app_yaml: None,
		};

		// Act
		let result = req.validate();

		// Assert
		assert_eq!(
			result.is_ok(),
			valid,
			"cluster_id={cluster_id} expected valid={valid}"
		);
	}

	/// Validate image length boundaries (min=1, max=512).
	#[rstest]
	#[case("n", true)]
	#[case("", false)]
	#[case(&"n".repeat(512), true)]
	#[case(&"n".repeat(513), false)]
	fn test_deployment_image_boundary(#[case] image: &str, #[case] valid: bool) {
		// Arrange
		let req = CreateDeploymentRequest {
			app_name: "my-app".to_string(),
			cluster_id: 1,
			image: image.to_string(),
			reinhardt_app_yaml: None,
		};

		// Act
		let result = req.validate();

		// Assert
		assert_eq!(
			result.is_ok(),
			valid,
			"image len={} expected valid={valid}",
			image.len()
		);
	}

	/// Missing required fields cause deserialization errors.
	#[rstest]
	#[case(r#"{"cluster_id": 1, "image": "nginx"}"#, "app_name")]
	#[case(r#"{"app_name": "web", "image": "nginx"}"#, "cluster_id")]
	#[case(r#"{"app_name": "web", "cluster_id": 1}"#, "image")]
	fn test_deployment_request_missing_fields(#[case] json: &str, #[case] missing_field: &str) {
		// Arrange & Act
		let result = serde_json::from_str::<CreateDeploymentRequest>(json);

		// Assert
		assert!(
			result.is_err(),
			"Should fail when {missing_field} is missing"
		);
		let err_msg = result.unwrap_err().to_string();
		assert!(
			err_msg.contains(missing_field),
			"Error should mention '{missing_field}', got: {err_msg}"
		);
	}
}
