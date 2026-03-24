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
/// - S3 backend with a role ARN: annotated with `eks.amazonaws.com/role-arn`
/// - GCS backend with a service account: annotated with `iam.gke.io/gcp-service-account`
/// - Backends without IAM values or non-cloud backends: returns `Ok(None)`
pub(crate) fn build_storage_service_account(
	app: &ReinhardtApp,
	backend: &str,
	iam_role: Option<&str>,
) -> Result<Option<ServiceAccount>, Error> {
	let annotations = match (backend, iam_role) {
		("s3", Some(role)) if !role.is_empty() => {
			BTreeMap::from([("eks.amazonaws.com/role-arn".to_string(), role.to_string())])
		}
		("gcs", Some(sa)) if !sa.is_empty() => {
			BTreeMap::from([("iam.gke.io/gcp-service-account".to_string(), sa.to_string())])
		}
		// No IAM role configured or non-cloud backend — skip ServiceAccount
		_ => return Ok(None),
	};

	let labels = standard_labels(app, Component::Web);
	let namespace = super::require_namespace(app)?;
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
		let sa =
			build_storage_service_account(&app, "s3", Some("arn:aws:iam::123456:role/my-role"))
				.expect("build should succeed")
				.expect("S3 with role should produce a ServiceAccount");

		// Assert
		let annotations = sa.metadata.annotations.as_ref().unwrap();
		assert_eq!(
			annotations.get("eks.amazonaws.com/role-arn").unwrap(),
			"arn:aws:iam::123456:role/my-role"
		);
	}

	#[rstest]
	fn test_build_storage_sa_gcs_annotation() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let sa = build_storage_service_account(
			&app,
			"gcs",
			Some("my-sa@project.iam.gserviceaccount.com"),
		)
		.expect("build should succeed")
		.expect("GCS with SA should produce a ServiceAccount");

		// Assert
		let annotations = sa.metadata.annotations.as_ref().unwrap();
		assert_eq!(
			annotations.get("iam.gke.io/gcp-service-account").unwrap(),
			"my-sa@project.iam.gserviceaccount.com"
		);
	}

	#[rstest]
	fn test_build_storage_sa_pvc_returns_none() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let result =
			build_storage_service_account(&app, "pvc", None).expect("build should succeed");

		// Assert
		assert!(result.is_none());
	}

	#[rstest]
	fn test_build_storage_sa_s3_without_role_returns_none() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let result = build_storage_service_account(&app, "s3", None).expect("build should succeed");

		// Assert
		assert!(result.is_none());
	}

	#[rstest]
	fn test_build_storage_sa_s3_with_empty_role_returns_none() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let result =
			build_storage_service_account(&app, "s3", Some("")).expect("build should succeed");

		// Assert
		assert!(result.is_none());
	}

	#[rstest]
	fn test_build_storage_sa_name() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let sa =
			build_storage_service_account(&app, "s3", Some("arn:aws:iam::123456:role/my-role"))
				.expect("build should succeed")
				.expect("S3 with role should produce a ServiceAccount");

		// Assert
		assert_eq!(sa.metadata.name.as_deref(), Some("myapp-storage"));
	}
}
