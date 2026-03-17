//! Tests for clusters app serializers.

#[cfg(test)]
mod tests {
	use rstest::rstest;
	use uuid::Uuid;

	use serde_json;

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
	fn test_cluster_response_with_none_id_serializes_to_null() {
		// Arrange
		let cluster = Cluster::new(
			"staging".to_string(),
			"https://staging.k8s.io:6443".to_string(),
			true,
		);

		// Act
		let resp = ClusterResponse::from(cluster);
		let json = serde_json::to_value(&resp).unwrap();

		// Assert
		assert_eq!(resp.id, None);
		assert!(json["id"].is_null());
	}

	#[rstest]
	fn test_cluster_response_with_some_id_serializes_to_number() {
		// Arrange
		let mut cluster = Cluster::new(
			"production".to_string(),
			"https://prod.k8s.io:6443".to_string(),
			true,
		);
		cluster.id = Some(42);

		// Act
		let resp = ClusterResponse::from(cluster);
		let json = serde_json::to_value(&resp).unwrap();

		// Assert
		assert_eq!(resp.id, Some(42));
		assert_eq!(json["id"], 42);
	}

	#[rstest]
	fn test_cluster_response_from_orm_model() {
		// Arrange
		let cluster = Cluster::new(
			Uuid::new_v4(),
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
