//! Property-based tests for deployment serializers.

#[cfg(test)]
mod tests {
	use proptest::prelude::*;

	use crate::apps::deployments::serializers::CreateDeploymentRequest;

	proptest! {
	/// CreateDeploymentRequest survives a JSON serialize/deserialize roundtrip.
	#[test]
		fn test_create_deployment_request_roundtrip(
			project_name in "[a-z]{1,63}",
			cluster_id in 1i64..=1_000_000,
			image in "[a-z0-9]{1,128}",
		) {
			// Arrange — build JSON string manually (CreateDeploymentRequest has no Serialize)
			let json = format!(
				r#"{{"project_name":"{}","cluster_id":{},"image":"{}"}}"#,
				project_name, cluster_id, image,
			);

			// Act
			let restored: CreateDeploymentRequest = serde_json::from_str(&json).expect("deserialize");

			// Assert
			prop_assert_eq!(&restored.project_name, &project_name);
			prop_assert_eq!(restored.cluster_id, cluster_id);
			prop_assert_eq!(&restored.image, &image);
		}

		/// Arbitrary JSON never causes a panic in CreateDeploymentRequest deserialization.
		#[test]
		fn test_arbitrary_json_deployment_no_panic(s in "\\PC{0,256}") {
			// Act & Assert — should never panic, only Ok or Err
			let _ = serde_json::from_str::<CreateDeploymentRequest>(&s);
		}
	}
}
