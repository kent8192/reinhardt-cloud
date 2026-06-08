//! Kubernetes resource operations for Reinhardt Cloud.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{Namespace, Secret};
use kube::api::{Patch, PatchParams};
use kube::{Api, ResourceExt};
use reinhardt_cloud_types::crd::ReinhardtApp;

use crate::client::{K8sError, KubeClient};

/// Parses and validates a `ReinhardtApp` YAML manifest.
pub fn parse_reinhardt_app_yaml(yaml: &str) -> Result<ReinhardtApp, K8sError> {
	let app: ReinhardtApp =
		serde_yaml::from_str(yaml).map_err(|e| K8sError::Manifest(e.to_string()))?;
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

/// Applies a `ReinhardtApp` YAML manifest using Kubernetes server-side apply.
pub async fn server_side_apply_reinhardt_app_yaml(
	client: &KubeClient,
	yaml: &str,
) -> Result<ReinhardtApp, K8sError> {
	let app = parse_reinhardt_app_yaml(yaml)?;
	let name = app.metadata.name.clone().ok_or(K8sError::MissingName)?;
	let namespace = app
		.metadata
		.namespace
		.clone()
		.unwrap_or_else(|| client.namespace().to_string());
	let api: Api<ReinhardtApp> = Api::namespaced(client.inner().clone(), &namespace);
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
	use rstest::rstest;

	use super::{K8sError, git_credentials_secret, parse_reinhardt_app_yaml};

	#[rstest]
	fn parse_reinhardt_app_yaml_accepts_valid_manifest() {
		// Arrange
		let yaml = r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: ReinhardtApp
metadata:
  name: demo
  namespace: default
spec:
  image: ghcr.io/example/demo:latest
"#;

		// Act
		let app = parse_reinhardt_app_yaml(yaml).expect("manifest should parse");

		// Assert
		assert_eq!(app.metadata.name.as_deref(), Some("demo"));
		assert_eq!(app.metadata.namespace.as_deref(), Some("default"));
		assert_eq!(app.spec.image, "ghcr.io/example/demo:latest");
	}

	#[rstest]
	fn parse_reinhardt_app_yaml_rejects_missing_name() {
		// Arrange
		let yaml = r#"
apiVersion: paas.reinhardt-cloud.dev/v1alpha2
kind: ReinhardtApp
metadata:
  namespace: default
spec:
  image: ghcr.io/example/demo:latest
"#;

		// Act
		let err = parse_reinhardt_app_yaml(yaml).expect_err("missing name should fail");

		// Assert
		assert!(matches!(err, K8sError::MissingName));
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
