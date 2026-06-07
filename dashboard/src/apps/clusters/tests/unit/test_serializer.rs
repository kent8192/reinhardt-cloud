//! Tests for clusters app serializers.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use serde_json;

	use crate::apps::clusters::serializers::CreateClusterRequest;

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
}
