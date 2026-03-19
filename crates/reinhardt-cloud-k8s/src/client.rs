//! Kubernetes client wrapper for Reinhardt Cloud.

use kube::config::InClusterError;
use kube::{Client, Config};
use thiserror::Error;

/// Errors from Kubernetes API operations.
#[derive(Debug, Error)]
pub enum K8sError {
	#[error("Failed to build Kubernetes client: {0}")]
	ClientBuild(#[from] kube::Error),

	#[error("Failed to load in-cluster config: {0}")]
	InCluster(#[from] InClusterError),

	#[error("Resource not found: {0}")]
	NotFound(String),

	#[error("API error: {0}")]
	Api(String),
}

/// Thin wrapper around the kube-rs `Client` with a default namespace.
#[derive(Clone)]
pub struct KubeClient {
	inner: Client,
	namespace: String,
}

impl KubeClient {
	/// Creates a client using in-cluster config (for production).
	pub async fn in_cluster(namespace: &str) -> Result<Self, K8sError> {
		let config = Config::incluster()?;
		let client = Client::try_from(config)?;
		Ok(Self {
			inner: client,
			namespace: namespace.to_string(),
		})
	}

	/// Creates a client using kubeconfig (for local development).
	pub async fn from_kubeconfig(namespace: &str) -> Result<Self, K8sError> {
		let client = Client::try_default().await?;
		Ok(Self {
			inner: client,
			namespace: namespace.to_string(),
		})
	}

	/// Returns a reference to the inner kube-rs client.
	pub fn inner(&self) -> &Client {
		&self.inner
	}

	/// Returns the default namespace.
	pub fn namespace(&self) -> &str {
		&self.namespace
	}
}
