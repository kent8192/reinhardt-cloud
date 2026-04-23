//! Unit tests for Cluster model construction and field behaviour.

#[cfg(test)]
mod tests {
	use rstest::rstest;
	use uuid::Uuid;

	use crate::apps::clusters::models::Cluster;

	#[rstest]
	fn test_cluster_new_sets_fields() {
		// Arrange
		let user_id = Uuid::now_v7();
		let name = "production".to_string();
		let api_url = "https://k8s.example.com:6443".to_string();

		// Act
		let cluster = Cluster::new(user_id, name.clone(), api_url.clone(), true, None, None);

		// Assert
		assert_eq!(cluster.user_id, user_id);
		assert_eq!(cluster.name, name);
		assert_eq!(cluster.api_url, api_url);
		assert!(cluster.is_active);
	}

	#[rstest]
	fn test_cluster_new_id_is_none() {
		// Arrange
		let user_id = Uuid::now_v7();

		// Act
		let cluster = Cluster::new(
			user_id,
			"test-cluster".to_string(),
			"https://k8s.example.com:6443".to_string(),
			true,
			None,
			None,
		);

		// Assert
		assert_eq!(cluster.id, None);
	}

	#[rstest]
	#[case(true)]
	#[case(false)]
	fn test_cluster_is_active_flag(#[case] active: bool) {
		// Arrange
		let user_id = Uuid::now_v7();

		// Act
		let cluster = Cluster::new(
			user_id,
			"flag-test".to_string(),
			"https://k8s.example.com:6443".to_string(),
			active,
			None,
			None,
		);

		// Assert
		assert_eq!(cluster.is_active, active);
	}

	#[rstest]
	fn test_cluster_serialization_roundtrip() {
		// Arrange
		let cluster = Cluster::new(
			Uuid::now_v7(),
			"roundtrip".to_string(),
			"https://k8s.example.com:6443".to_string(),
			true,
			None,
			None,
		);

		// Act
		let json = serde_json::to_string(&cluster).expect("serialize should succeed");
		let deserialized: Cluster =
			serde_json::from_str(&json).expect("deserialize should succeed");

		// Assert
		assert_eq!(deserialized.name, cluster.name);
		assert_eq!(deserialized.api_url, cluster.api_url);
		assert_eq!(deserialized.user_id, cluster.user_id);
		assert_eq!(deserialized.is_active, cluster.is_active);
		assert_eq!(deserialized.id, cluster.id);
	}
}
