//! Unit tests for the Deployment model.

#[cfg(test)]
mod tests {
	use rstest::rstest;
	use uuid::Uuid;

	use crate::apps::deployments::models::Deployment;

	/// All fields from Deployment::new match constructor arguments.
	#[rstest]
	fn test_deployment_new_sets_fields() {
		// Arrange
		let user_id = Uuid::new_v4();
		let app_name = "my-app".to_string();
		let cluster_id = 42i64;
		let status = "pending".to_string();
		let image = "ghcr.io/my-app:latest".to_string();

		// Act
		let deployment = Deployment::new(
			user_id,
			app_name.clone(),
			cluster_id,
			status.clone(),
			image.clone(),
		);

		// Assert
		assert_eq!(deployment.user_id, user_id);
		assert_eq!(deployment.app_name, app_name);
		assert_eq!(deployment.cluster_id, cluster_id);
		assert_eq!(deployment.status, status);
		assert_eq!(deployment.image, image);
	}

	/// Deployment::new sets id to None (auto-increment on insert).
	#[rstest]
	fn test_deployment_new_id_is_none() {
		// Arrange & Act
		let deployment = Deployment::new(
			Uuid::new_v4(),
			"app".to_string(),
			1,
			"pending".to_string(),
			"nginx:latest".to_string(),
		);

		// Assert
		assert_eq!(deployment.id, None);
	}

	/// Deployment accepts various status string values.
	#[rstest]
	#[case("pending")]
	#[case("running")]
	#[case("failed")]
	#[case("succeeded")]
	fn test_deployment_status_values(#[case] status: &str) {
		// Arrange & Act
		let deployment = Deployment::new(
			Uuid::new_v4(),
			"app".to_string(),
			1,
			status.to_string(),
			"nginx:latest".to_string(),
		);

		// Assert
		assert_eq!(deployment.status, status);
	}

	/// Deployment survives a serde_json roundtrip.
	#[rstest]
	fn test_deployment_serialization_roundtrip() {
		// Arrange
		let mut deployment = Deployment::new(
			Uuid::new_v4(),
			"roundtrip-app".to_string(),
			99,
			"running".to_string(),
			"ghcr.io/roundtrip:v1".to_string(),
		);
		deployment.id = Some(7);

		// Act
		let json = serde_json::to_string(&deployment).expect("serialize");
		let restored: Deployment = serde_json::from_str(&json).expect("deserialize");

		// Assert
		assert_eq!(restored.id, deployment.id);
		assert_eq!(restored.user_id, deployment.user_id);
		assert_eq!(restored.app_name, deployment.app_name);
		assert_eq!(restored.cluster_id, deployment.cluster_id);
		assert_eq!(restored.status, deployment.status);
		assert_eq!(restored.image, deployment.image);
	}
}
