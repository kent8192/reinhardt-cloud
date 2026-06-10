//! i18n locale ConfigMap builder.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::ConfigMap;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use reinhardt_cloud_types::crd::Project;

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;

/// Build a ConfigMap for i18n locale configuration.
///
/// Creates a ConfigMap with empty data that serves as a mount point
/// for locale files. Users populate it with translation files via kubectl.
pub(crate) fn build_i18n_configmap(app: &Project) -> Result<ConfigMap, Error> {
	let namespace = super::require_namespace(app)?;
	let name = app.name_any();

	Ok(ConfigMap {
		metadata: ObjectMeta {
			name: Some(format!("{}-locales", name)),
			namespace: Some(namespace),
			labels: Some(standard_labels(app, Component::Web)),
			owner_references: Some(vec![owner_reference(app)?]),
			..Default::default()
		},
		data: Some(BTreeMap::new()),
		..Default::default()
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use kube::api::ObjectMeta;
	use reinhardt_cloud_types::crd::ProjectSpec;
	use rstest::rstest;

	fn make_test_app(name: &str) -> Project {
		Project {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				..Default::default()
			},
			spec: ProjectSpec {
				image: "img:v1".to_string(),
				..Default::default()
			},
			status: None,
		}
	}

	#[rstest]
	fn test_build_i18n_configmap_name() {
		// Arrange
		let app = make_test_app("my-app");

		// Act
		let cm = build_i18n_configmap(&app).expect("build should succeed");

		// Assert
		assert_eq!(cm.metadata.name.as_deref(), Some("my-app-locales"));
	}

	#[rstest]
	fn test_build_i18n_configmap_empty_data() {
		// Arrange
		let app = make_test_app("my-app");

		// Act
		let cm = build_i18n_configmap(&app).expect("build should succeed");

		// Assert
		let data = cm.data.expect("data should be Some");
		assert!(data.is_empty());
	}
}
