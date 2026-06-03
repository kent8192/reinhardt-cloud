//! Property-based tests for Cluster model and serializers.

#[cfg(test)]
mod tests {
	use proptest::prelude::*;

	use crate::apps::clusters::models::Cluster;
	use crate::apps::clusters::serializers::{ClusterResponse, CreateClusterRequest};

	proptest! {
		/// ClusterResponse::from preserves all fields from the Cluster model.
		#[test]
		fn test_cluster_response_from_model_preserves_fields(
			name in "[a-zA-Z0-9_-]{1,63}",
			api_url in "https://[a-z]{1,20}\\.example\\.com(:[0-9]{1,5})?",
			is_active in proptest::bool::ANY,
		) {
			// Arrange
			let cluster = Cluster::build()
				.organization_id(1i64)
				.name(name.clone())
				.api_url(api_url.clone())
				.is_active(is_active)
				.token_hash(None)
				.token_last_rotated_at(None)
				.finish();

			// Act
			let resp = ClusterResponse::from(cluster);

			// Assert
			prop_assert_eq!(&resp.name, &name);
			prop_assert_eq!(&resp.api_url, &api_url);
			prop_assert_eq!(resp.is_active, is_active);
			prop_assert_eq!(resp.id, None);
		}

		/// CreateClusterRequest deserializes correctly from well-formed JSON.
		#[test]
		fn test_create_cluster_request_roundtrip(
			name in "[a-zA-Z0-9_-]{1,63}",
			api_url in "https://[a-z]{1,20}\\.example\\.com(:[0-9]{1,5})?",
		) {
			// Arrange
			let json = serde_json::json!({
				"name": name,
				"api_url": api_url,
			});

			// Act
			let deserialized: CreateClusterRequest =
				serde_json::from_value(json).expect("deserialize should succeed");

			// Assert
			prop_assert_eq!(&deserialized.name, &name);
			prop_assert_eq!(&deserialized.api_url, &api_url);
		}

		/// Deserializing arbitrary JSON into CreateClusterRequest must never panic.
		#[test]
		fn test_arbitrary_json_cluster_no_panic(json_str in "\\PC{0,256}") {
			// Arrange / Act
			let _ = serde_json::from_str::<CreateClusterRequest>(&json_str);

			// Assert — no panic is the success condition
		}
	}
}
