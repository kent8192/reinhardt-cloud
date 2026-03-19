//! Storage ServiceAccount builders for cloud IAM integration.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::ServiceAccount;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use reinhardt_cloud_types::crd::ReinhardtApp;

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;

/// Builds a `ServiceAccount` with cloud IAM annotations for the given storage backend.
///
/// - S3 backend: annotated with `eks.amazonaws.com/role-arn` (placeholder)
/// - GCS backend: annotated with `iam.gke.io/gcp-service-account` (placeholder)
/// - Other backends (e.g., `pvc`): returns `Ok(None)` (no ServiceAccount needed)
pub(crate) fn build_storage_service_account(
	app: &ReinhardtApp,
	backend: &str,
) -> Result<Option<ServiceAccount>, Error> {
	let annotations = match backend {
		"s3" => BTreeMap::from([("eks.amazonaws.com/role-arn".to_string(), String::new())]),
		"gcs" => BTreeMap::from([("iam.gke.io/gcp-service-account".to_string(), String::new())]),
		_ => return Ok(None),
	};

	let labels = standard_labels(app, Component::Web);
	let namespace = app.namespace().unwrap_or_default();
	let owner_ref = owner_reference(app)?;
	let app_name = app.name_any();

	Ok(Some(ServiceAccount {
		metadata: ObjectMeta {
			name: Some(format!("{}-storage", app_name)),
			namespace: Some(namespace),
			labels: Some(labels),
			annotations: Some(annotations),
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		..Default::default()
	}))
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	fn test_app(name: &str) -> ReinhardtApp {
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": name, "namespace": "default", "uid": "test-uid" },
			"spec": { "image": "myapp:latest" }
		});
		serde_json::from_value(json).unwrap()
	}

	#[rstest]
	fn test_build_storage_sa_s3_annotation() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let sa = build_storage_service_account(&app, "s3")
			.expect("build should succeed")
			.expect("S3 should produce a ServiceAccount");

		// Assert
		let annotations = sa.metadata.annotations.as_ref().unwrap();
		assert!(annotations.contains_key("eks.amazonaws.com/role-arn"));
	}

	#[rstest]
	fn test_build_storage_sa_gcs_annotation() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let sa = build_storage_service_account(&app, "gcs")
			.expect("build should succeed")
			.expect("GCS should produce a ServiceAccount");

		// Assert
		let annotations = sa.metadata.annotations.as_ref().unwrap();
		assert!(annotations.contains_key("iam.gke.io/gcp-service-account"));
	}

	#[rstest]
	fn test_build_storage_sa_pvc_returns_none() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let result = build_storage_service_account(&app, "pvc").expect("build should succeed");

		// Assert
		assert!(result.is_none());
	}

	#[rstest]
	fn test_build_storage_sa_name() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let sa = build_storage_service_account(&app, "s3")
			.expect("build should succeed")
			.expect("S3 should produce a ServiceAccount");

		// Assert
		assert_eq!(sa.metadata.name.as_deref(), Some("myapp-storage"));
	}
}
