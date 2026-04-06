//! Property-based tests for deployment serializers.

#[cfg(test)]
mod tests {
	use proptest::prelude::*;
	use uuid::Uuid;

	use crate::apps::deployments::models::Deployment;
	use crate::apps::deployments::serializers::{CreateDeploymentRequest, DeploymentResponse};

	/// Strategy for generating arbitrary Deployment instances.
	fn arb_deployment() -> impl Strategy<Value = Deployment> {
		(
			prop::array::uniform16(any::<u8>()),
			"[a-z][a-z0-9\\-]{0,62}",
			1i64..=i64::MAX,
			prop::sample::select(vec![
				"pending".to_string(),
				"running".to_string(),
				"failed".to_string(),
				"succeeded".to_string(),
			]),
			"[a-z0-9./:\\-]{1,128}",
		)
			.prop_map(|(uuid_bytes, app_name, cluster_id, status, image)| {
				let mut d = Deployment::new(
					Uuid::from_bytes(uuid_bytes),
					app_name,
					cluster_id,
					status,
					image,
				);
				d.id = Some(cluster_id);
				d
			})
	}

	proptest! {
		/// DeploymentResponse::from preserves all fields from the source Deployment.
		#[test]
		fn test_deployment_response_preserves_fields(
			(expected_id, expected_app, expected_cluster, expected_status, expected_image, deployment)
			in arb_deployment().prop_map(|d| {
				let id = d.id;
				let app = d.app_name.clone();
				let cluster = d.cluster_id;
				let status = d.status.clone();
				let image = d.image.clone();
				(id, app, cluster, status, image, d)
			})
		) {
			// Act
			let response = DeploymentResponse::from(deployment);

			// Assert
			prop_assert_eq!(response.id, expected_id);
			prop_assert_eq!(&response.app_name, &expected_app);
			prop_assert_eq!(response.cluster_id, expected_cluster);
			prop_assert_eq!(&response.status, &expected_status);
			prop_assert_eq!(&response.image, &expected_image);
		}

		/// CreateDeploymentRequest survives a JSON serialize/deserialize roundtrip.
		#[test]
		fn test_create_deployment_request_roundtrip(
			app_name in "[a-z]{1,63}",
			cluster_id in 1i64..=1_000_000,
			image in "[a-z0-9]{1,128}",
		) {
			// Arrange — build JSON string manually (CreateDeploymentRequest has no Serialize)
			let json = format!(
				r#"{{"app_name":"{}","cluster_id":{},"image":"{}"}}"#,
				app_name, cluster_id, image,
			);

			// Act
			let restored: CreateDeploymentRequest = serde_json::from_str(&json).expect("deserialize");

			// Assert
			prop_assert_eq!(&restored.app_name, &app_name);
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
