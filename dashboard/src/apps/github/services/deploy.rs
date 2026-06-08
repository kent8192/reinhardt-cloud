//! Kubernetes apply helpers for GitHub-imported projects.

use reinhardt_cloud_k8s::KubeClient;
use reinhardt_cloud_k8s::resources::{
	parse_reinhardt_app_yaml, server_side_apply_reinhardt_app_yaml,
};

pub async fn apply_reinhardt_app_yaml(yaml: &str) -> Result<(), String> {
	let app = parse_reinhardt_app_yaml(yaml).map_err(|e| e.to_string())?;
	let namespace = app.metadata.namespace.as_deref().unwrap_or("default");
	let client = match KubeClient::in_cluster(namespace).await {
		Ok(client) => client,
		Err(in_cluster_error) => KubeClient::from_kubeconfig(namespace)
			.await
			.map_err(|kubeconfig_error| {
				format!(
					"Failed to build Kubernetes client from in-cluster config ({in_cluster_error}) or kubeconfig ({kubeconfig_error})"
				)
			})?,
	};
	server_side_apply_reinhardt_app_yaml(&client, yaml)
		.await
		.map(|_| ())
		.map_err(|e| e.to_string())
}
