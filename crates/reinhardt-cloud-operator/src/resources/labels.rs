//! Standard labels and owner references for operator-managed resources.

use std::collections::BTreeMap;

use k8s_openapi::apimachinery::pkg::apis::meta::v1::OwnerReference;
use kube::{Resource, ResourceExt};
use reinhardt_cloud_types::crd::ReinhardtApp;

use crate::error::Error;

/// Identifies which logical component a Kubernetes resource belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Component {
	/// Web-facing application container.
	Web,
	/// Background worker container.
	Worker,
	/// Database instance.
	Database,
	/// Cache instance (e.g. Redis).
	Cache,
	/// Ingress resource.
	Ingress,
	/// Database migration job.
	Migration,
	/// Static file server sidecar (e.g. static-web-server for WASM assets).
	/// Currently used for label generation in future per-resource labeling.
	#[allow(dead_code)]
	StaticServer,
	/// Container image build job (e.g. Kaniko).
	Build,
	/// Preview environment for pull requests.
	#[allow(dead_code)]
	Preview,
}

impl Component {
	/// Returns the label value for `app.kubernetes.io/component`.
	pub(crate) fn as_str(&self) -> &'static str {
		match self {
			Self::Web => "web",
			Self::Worker => "worker",
			Self::Database => "database",
			Self::Cache => "cache",
			Self::Ingress => "ingress",
			Self::Migration => "migration",
			Self::StaticServer => "static-server",
			Self::Build => "build",
			Self::Preview => "preview",
		}
	}
}

/// Standard labels applied to all resources owned by the operator.
///
/// Includes `app.kubernetes.io/component` based on the given [`Component`].
pub(crate) fn standard_labels(
	app: &ReinhardtApp,
	component: Component,
) -> BTreeMap<String, String> {
	BTreeMap::from([
		("app.kubernetes.io/name".to_string(), app.name_any()),
		(
			"app.kubernetes.io/managed-by".to_string(),
			"reinhardt-cloud-operator".to_string(),
		),
		("app.kubernetes.io/instance".to_string(), app.name_any()),
		(
			"app.kubernetes.io/component".to_string(),
			component.as_str().to_string(),
		),
		(
			"paas.reinhardt-cloud.dev/owner".to_string(),
			format!(
				"{}.{}",
				app.namespace().unwrap_or_else(|| "<unknown>".to_string()),
				app.name_any()
			),
		),
	])
}

/// Computes the controller owner reference for the given `ReinhardtApp`.
pub(crate) fn owner_reference(app: &ReinhardtApp) -> Result<OwnerReference, Error> {
	app.controller_owner_ref(&())
		.ok_or_else(|| Error::OwnerReference(app.name_any()))
}

#[cfg(test)]
mod tests {
	use super::*;
	use kube::api::ObjectMeta;
	use reinhardt_cloud_types::crd::ReinhardtAppSpec;
	use rstest::rstest;

	fn make_test_app(name: &str) -> ReinhardtApp {
		ReinhardtApp {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				..Default::default()
			},
			spec: ReinhardtAppSpec {
				image: "img:v1".to_string(),
				..Default::default()
			},
			status: None,
		}
	}

	#[rstest]
	fn test_standard_labels_includes_required_keys() {
		// Arrange
		let app = make_test_app("my-app");

		// Act
		let labels = standard_labels(&app, Component::Web);

		// Assert
		assert_eq!(labels.get("app.kubernetes.io/name").unwrap(), "my-app");
		assert_eq!(
			labels.get("app.kubernetes.io/managed-by").unwrap(),
			"reinhardt-cloud-operator"
		);
		assert_eq!(labels.get("app.kubernetes.io/instance").unwrap(), "my-app");
		assert_eq!(
			labels.get("paas.reinhardt-cloud.dev/owner").unwrap(),
			"default.my-app"
		);
	}

	#[rstest]
	fn test_standard_labels_includes_component_label() {
		// Arrange
		let app = make_test_app("my-app");

		// Act
		let labels = standard_labels(&app, Component::Web);

		// Assert
		assert_eq!(labels.get("app.kubernetes.io/component").unwrap(), "web");
	}

	#[rstest]
	#[case(Component::Web, "web")]
	#[case(Component::Worker, "worker")]
	#[case(Component::Database, "database")]
	#[case(Component::Cache, "cache")]
	#[case(Component::Ingress, "ingress")]
	#[case(Component::Migration, "migration")]
	#[case(Component::StaticServer, "static-server")]
	#[case(Component::Build, "build")]
	#[case(Component::Preview, "preview")]
	fn test_component_as_str(#[case] component: Component, #[case] expected: &str) {
		// Arrange / Act / Assert
		assert_eq!(component.as_str(), expected);
	}

	#[rstest]
	fn test_owner_reference_succeeds_with_uid() {
		// Arrange
		let app = make_test_app("my-app");

		// Act
		let result = owner_reference(&app);

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn test_owner_reference_fails_without_uid() {
		// Arrange
		let mut app = make_test_app("my-app");
		app.metadata.uid = None;

		// Act
		let result = owner_reference(&app);

		// Assert
		assert!(result.is_err());
	}
}
