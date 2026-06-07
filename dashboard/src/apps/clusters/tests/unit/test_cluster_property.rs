//! Property-based tests for Cluster model and serializers.

#[cfg(test)]
mod tests {
	use proptest::prelude::*;

	use crate::apps::clusters::serializers::CreateClusterRequest;

	proptest! {
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
