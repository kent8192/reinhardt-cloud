//! Unit tests for the Deployment model.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use crate::apps::deployments::models::Deployment;

	/// All fields from Deployment::new match constructor arguments.
	#[rstest]
	fn test_deployment_new_sets_fields() {
		// Arrange
		let organization_id: i64 = 42;
		let app_name = "my-app".to_string();
		let cluster_id = 42i64;
		let status = "pending".to_string();
		let image = "ghcr.io/my-app:latest".to_string();
		let reinhardt_app_yaml =
			"apiVersion: paas.reinhardt-cloud.dev/v1alpha2\nkind: ReinhardtApp\n".to_string();

		// Act
		let deployment = Deployment::build()
			.organization_id(organization_id)
			.app_name(app_name.clone())
			.cluster_id(cluster_id)
			.status(status.clone())
			.image(image.clone())
			.reinhardt_app_yaml(Some(reinhardt_app_yaml.clone()))
			.finish();

		// Assert
		assert_eq!(deployment.organization_id, organization_id);
		assert_eq!(deployment.app_name, app_name);
		assert_eq!(deployment.cluster_id, cluster_id);
		assert_eq!(deployment.status, status);
		assert_eq!(deployment.image, image);
		assert_eq!(deployment.reinhardt_app_yaml, Some(reinhardt_app_yaml));
	}

	/// Deployment::new sets id to None (auto-increment on insert).
	#[rstest]
	fn test_deployment_new_id_is_none() {
		// Arrange & Act
		let deployment = Deployment::build()
			.organization_id(1)
			.app_name("app".to_string())
			.cluster_id(1)
			.status("pending".to_string())
			.image("nginx:latest".to_string())
			.reinhardt_app_yaml(None)
			.finish();

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
		let deployment = Deployment::build()
			.organization_id(1)
			.app_name("app".to_string())
			.cluster_id(1)
			.status(status.to_string())
			.image("nginx:latest".to_string())
			.reinhardt_app_yaml(None)
			.finish();

		// Assert
		assert_eq!(deployment.status, status);
	}

	/// Deployment survives a serde_json roundtrip.
	#[rstest]
	fn test_deployment_serialization_roundtrip() {
		// Arrange
		let mut deployment = Deployment::build()
			.organization_id(7)
			.app_name("roundtrip-app".to_string())
			.cluster_id(99)
			.status("running".to_string())
			.image("ghcr.io/roundtrip:v1".to_string())
			.reinhardt_app_yaml(None)
			.finish();
		deployment.id = Some(7);

		// Act
		let json = serde_json::to_string(&deployment).expect("serialize");
		let restored: Deployment = serde_json::from_str(&json).expect("deserialize");

		// Assert
		assert_eq!(restored.id, deployment.id);
		assert_eq!(restored.organization_id, deployment.organization_id);
		assert_eq!(restored.app_name, deployment.app_name);
		assert_eq!(restored.cluster_id, deployment.cluster_id);
		assert_eq!(restored.status, deployment.status);
		assert_eq!(restored.image, deployment.image);
		assert_eq!(restored.reinhardt_app_yaml, deployment.reinhardt_app_yaml);
	}
}
