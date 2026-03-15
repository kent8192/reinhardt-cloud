//! Kubernetes resource operations for nuages.

use k8s_openapi::api::core::v1::Namespace;
use kube::{Api, ResourceExt};

use crate::client::{K8sError, KubeClient};

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
