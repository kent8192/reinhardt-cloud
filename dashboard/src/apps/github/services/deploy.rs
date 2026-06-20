//! Kubernetes apply helpers for GitHub-imported projects.

use std::sync::Arc;

use reinhardt_cloud_k8s::resources::{
	parse_project_yaml, server_side_apply_git_credentials_secret, server_side_apply_project_yaml,
};
use reinhardt_cloud_types::agent::AgentCommand;

use crate::apps::clusters::models::Cluster;
pub use crate::apps::deployments::services::agent::{
	cluster_uuid_from_pk, send_project_apply_to_cluster, validate_cluster_for_apply,
};

/// Builds a Kubernetes client for the namespace that owns a Project apply target.
pub(crate) async fn kube_client_for_namespace(
	namespace: &str,
) -> Result<reinhardt_cloud_k8s::KubeClient, String> {
	Ok(
		match reinhardt_cloud_k8s::KubeClient::in_cluster(namespace).await {
			Ok(client) => client,
			Err(in_cluster_error) => {
				let in_cluster_error = in_cluster_error.to_string();
				reinhardt_cloud_k8s::KubeClient::from_kubeconfig(namespace)
					.await
					.map_err(|kubeconfig_error| {
						let kubeconfig_error = kubeconfig_error.to_string();
						format!(
							"Failed to build Kubernetes client from in-cluster config ({}) or kubeconfig ({})",
							in_cluster_error, kubeconfig_error
						)
					})?
			}
		},
	)
}

pub async fn send_git_credentials_secret_to_cluster(
	registry: &Arc<reinhardt_cloud_grpc::registry::AgentRegistry>,
	cluster: &Cluster,
	project_name: &str,
	namespace: &str,
	secret_name: &str,
	git_token: &str,
) -> Result<(), String> {
	validate_cluster_for_apply(cluster)?;
	let cluster_id = cluster_uuid_from_pk(cluster.id)?;
	registry
		.send_command_to_cluster(
			&cluster_id,
			AgentCommand::ApplyGitCredentialsSecret {
				project_name: project_name.to_string(),
				namespace: namespace.to_string(),
				secret_name: secret_name.to_string(),
				git_token: reinhardt_cloud_types::agent::SecretString::new(git_token.to_string()),
			},
		)
		.await
		.map_err(|e| {
			format!(
				"Failed to route Git credentials Secret apply to cluster '{}': {e}",
				cluster.name
			)
		})
}

pub async fn apply_project_yaml(yaml: &str) -> Result<(), String> {
	let app = parse_project_yaml(yaml).map_err(|e| e.to_string())?;
	let namespace = app.metadata.namespace.as_deref().unwrap_or("default");
	let client = kube_client_for_namespace(namespace).await?;
	server_side_apply_project_yaml(&client, yaml)
		.await
		.map(|_| ())
		.map_err(|e| e.to_string())
}

pub async fn apply_project_yaml_for_cluster(yaml: &str, cluster: &Cluster) -> Result<(), String> {
	validate_cluster_for_apply(cluster)?;
	apply_project_yaml(yaml).await.map_err(|e| {
		format!(
			"Failed to apply manifest for cluster '{}': {e}",
			cluster.name
		)
	})
}

pub async fn apply_git_credentials_secret_for_cluster(
	namespace: &str,
	secret_name: &str,
	git_token: &str,
	cluster: &Cluster,
) -> Result<(), String> {
	validate_cluster_for_apply(cluster)?;
	let client = kube_client_for_namespace(namespace).await?;
	server_side_apply_git_credentials_secret(&client, namespace, secret_name, git_token)
		.await
		.map(|_| ())
		.map_err(|e| {
			format!(
				"Failed to apply git credentials Secret for cluster '{}': {e}",
				cluster.name
			)
		})
}
