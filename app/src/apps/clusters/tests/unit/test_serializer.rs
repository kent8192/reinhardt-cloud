//! Tests for clusters app serializers.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use crate::apps::clusters::models::Cluster;
	use crate::apps::clusters::serializers::{ClusterResponse, CreateClusterRequest};

	#[rstest]
	fn test_create_cluster_request_deserializes() {
		// Arrange
		let json = r#"{"name": "prod", "api_url": "https://k8s.example.com:6443"}"#;

		// Act
		let req: CreateClusterRequest = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(req.name, "prod");
		assert_eq!(req.api_url, "https://k8s.example.com:6443");
	}

	#[rstest]
	fn test_cluster_response_from_orm_model() {
		// Arrange
		let cluster = Cluster::new(
			"staging".to_string(),
			"https://staging.k8s.io:6443".to_string(),
			true,
		);

		// Act
		let resp = ClusterResponse::from(cluster);

		// Assert
		assert_eq!(resp.name, "staging");
		assert_eq!(resp.api_url, "https://staging.k8s.io:6443");
		assert!(resp.is_active);
	}
}
