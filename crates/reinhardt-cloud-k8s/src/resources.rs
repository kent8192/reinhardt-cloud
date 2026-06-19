//! Kubernetes resource operations for Reinhardt Cloud.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{Namespace, Secret};
use kube::api::{GetParams, Patch, PatchParams};
use kube::{Api, ResourceExt};
use reinhardt_cloud_types::crd::Project;

use crate::client::{K8sError, KubeClient};

/// Parses and validates a `Project` YAML manifest.
pub fn parse_project_yaml(yaml: &str) -> Result<Project, K8sError> {
	let app: Project = serde_yaml::from_str(yaml).map_err(|e| K8sError::Manifest(e.to_string()))?;
	if app.metadata.name.as_deref().is_none_or(str::is_empty) {
		return Err(K8sError::MissingName);
	}
	if let Err(errors) = app.spec.validate() {
		let messages = errors
			.into_iter()
			.map(|e| e.message)
			.collect::<Vec<_>>()
			.join("; ");
		return Err(K8sError::Validation(messages));
	}
	Ok(app)
}

/// Reads a `Project` from Kubernetes by namespace and name.
pub async fn get_project(
	client: &KubeClient,
	namespace: &str,
	name: &str,
) -> Result<Project, K8sError> {
	Api::<Project>::namespaced(client.inner().clone(), namespace)
		.get_with(name, &GetParams::default())
		.await
		.map_err(|error| map_project_get_error(namespace, name, error))
}

fn map_project_get_error(namespace: &str, name: &str, error: kube::Error) -> K8sError {
	match error {
		kube::Error::Api(status) if status.code == 404 => {
			K8sError::NotFound(project_resource_key(namespace, name))
		}
		error => K8sError::Api(error.to_string()),
	}
}

fn project_resource_key(namespace: &str, name: &str) -> String {
	format!("{namespace}/{name}")
}

/// Applies a `Project` YAML manifest using Kubernetes server-side apply.
pub async fn server_side_apply_project_yaml(
	client: &KubeClient,
	yaml: &str,
) -> Result<Project, K8sError> {
	let app = parse_project_yaml(yaml)?;
	let name = app.metadata.name.clone().ok_or(K8sError::MissingName)?;
	let namespace = app
		.metadata
		.namespace
		.clone()
		.unwrap_or_else(|| client.namespace().to_string());
	let api: Api<Project> = Api::namespaced(client.inner().clone(), &namespace);
	api.patch(
		&name,
		&PatchParams::apply("reinhardt-cloud-dashboard").force(),
		&Patch::Apply(&app),
	)
	.await
	.map_err(|e| K8sError::Api(e.to_string()))
}

/// Applies a Git credential Secret used by `spec.source.credentials_secret`.
pub async fn server_side_apply_git_credentials_secret(
	client: &KubeClient,
	namespace: &str,
	name: &str,
	git_token: &str,
) -> Result<Secret, K8sError> {
	let api: Api<Secret> = Api::namespaced(client.inner().clone(), namespace);
	let secret = git_credentials_secret(namespace, name, git_token);
	api.patch(
		name,
		&PatchParams::apply("reinhardt-cloud-dashboard").force(),
		&Patch::Apply(&secret),
	)
	.await
	.map_err(|e| K8sError::Api(e.to_string()))
}

fn git_credentials_secret(namespace: &str, name: &str, git_token: &str) -> Secret {
	let mut labels = BTreeMap::new();
	labels.insert(
		"reinhardt.dev/credential-type".to_string(),
		"git".to_string(),
	);
	labels.insert("reinhardt.dev/provider".to_string(), "github".to_string());

	let mut string_data = BTreeMap::new();
	string_data.insert("git-token".to_string(), git_token.to_string());

	Secret {
		metadata: kube::api::ObjectMeta {
			name: Some(name.to_string()),
			namespace: Some(namespace.to_string()),
			labels: Some(labels),
			..Default::default()
		},
		type_: Some("Opaque".to_string()),
		string_data: Some(string_data),
		..Default::default()
	}
}

/// Client for Kubernetes Namespace operations.
pub struct NamespaceClient<'a> {
	client: &'a KubeClient,
}

impl<'a> NamespaceClient<'a> {
	/// Creates a new namespace client.
	pub fn new(client: &'a KubeClient) -> Self {
		Self { client }
	}

	/// Lists all namespace names in the cluster.
	pub async fn list(&self) -> Result<Vec<String>, K8sError> {
		let api: Api<Namespace> = Api::all(self.client.inner().clone());
		let ns_list = api
			.list(&Default::default())
			.await
			.map_err(|e| K8sError::Api(e.to_string()))?;
		Ok(ns_list.items.iter().map(|ns| ns.name_any()).collect())
	}
}

#[cfg(test)]
mod tests {
	use kube::core::Status;
	use rstest::rstest;

	use super::{
		K8sError, git_credentials_secret, map_project_get_error, parse_project_yaml,
		project_resource_key,
	};

	#[rstest]
	fn parse_project_yaml_accepts_valid_manifest() {
		// Arrange
		let yaml = r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: Project
metadata:
  name: demo
  namespace: default
spec:
  image: ghcr.io/example/demo:latest
"#;

		// Act
		let app = parse_project_yaml(yaml).expect("manifest should parse");

		// Assert
		assert_eq!(app.metadata.name.as_deref(), Some("demo"));
		assert_eq!(app.metadata.namespace.as_deref(), Some("default"));
		assert_eq!(app.spec.image, "ghcr.io/example/demo:latest");
	}

	#[rstest]
	fn parse_project_yaml_rejects_missing_name() {
		// Arrange
		let yaml = r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: Project
metadata:
  namespace: default
spec:
  image: ghcr.io/example/demo:latest
"#;

		// Act
		let err = parse_project_yaml(yaml).expect_err("missing name should fail");

		// Assert
		assert!(matches!(err, K8sError::MissingName));
	}

	#[rstest]
	#[case("default", "demo", "default/demo")]
	#[case("demo-preview", "demo-pr-12", "demo-preview/demo-pr-12")]
	fn project_resource_key_formats_namespace_and_name(
		#[case] namespace: &str,
		#[case] name: &str,
		#[case] expected: &str,
	) {
		// Arrange / Act
		let resource_key = project_resource_key(namespace, name);

		// Assert
		assert_eq!(resource_key, expected);
	}

	#[rstest]
	fn map_project_get_error_maps_404_to_not_found() {
		// Arrange
		let error = kube::Error::Api(
			Status::failure(
				"projects.paas.reinhardt-cloud.dev \"demo\" not found",
				"NotFound",
			)
			.with_code(404)
			.boxed(),
		);

		// Act
		let err = map_project_get_error("apps", "demo", error);

		// Assert
		match err {
			K8sError::NotFound(resource) => assert_eq!(resource, "apps/demo"),
			other => panic!("expected NotFound, got {other:?}"),
		}
	}

	#[rstest]
	fn map_project_get_error_maps_non_404_to_api_error() {
		// Arrange
		let error = kube::Error::Api(
			Status::failure("project read is forbidden", "Forbidden")
				.with_code(403)
				.boxed(),
		);
		let expected = error.to_string();

		// Act
		let err = map_project_get_error("apps", "demo", error);

		// Assert
		match err {
			K8sError::Api(message) => assert_eq!(message, expected),
			other => panic!("expected Api error, got {other:?}"),
		}
	}

	#[rstest]
	fn git_credentials_secret_sets_name_namespace_labels_and_token() {
		// Arrange / Act
		let secret = git_credentials_secret("apps", "my-app-github-git-credentials", "ghs_token");

		// Assert
		assert_eq!(
			secret.metadata.name.as_deref(),
			Some("my-app-github-git-credentials")
		);
		assert_eq!(secret.metadata.namespace.as_deref(), Some("apps"));
		let labels = secret.metadata.labels.expect("labels should be set");
		assert_eq!(
			labels
				.get("reinhardt.dev/credential-type")
				.map(String::as_str),
			Some("git")
		);
		assert_eq!(
			labels.get("reinhardt.dev/provider").map(String::as_str),
			Some("github")
		);
		let string_data = secret.string_data.expect("stringData should be set");
		assert_eq!(
			string_data.get("git-token").map(String::as_str),
			Some("ghs_token")
		);
	}
}
