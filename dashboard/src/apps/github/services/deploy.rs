//! Kubernetes apply helpers for GitHub-imported projects.

use std::sync::Arc;

use reinhardt_cloud_k8s::KubeClient;
use reinhardt_cloud_k8s::resources::{
	parse_project_yaml, server_side_apply_git_credentials_secret,
	server_side_apply_project_yaml,
};
use reinhardt_cloud_types::agent::AgentCommand;
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;

fn validate_cluster_for_apply(cluster: &Cluster) -> Result<(), String> {
	if !cluster.is_active {
		return Err(format!("Cluster '{}' is not active", cluster.name));
	}
	if cluster.api_url.trim().is_empty() {
		return Err(format!(
			"Cluster '{}' has no Kubernetes API URL",
			cluster.name
		));
	}
	Ok(())
}

pub fn cluster_uuid_from_pk(id: Option<i64>) -> Result<Uuid, String> {
	let pk = id.ok_or_else(|| "Cluster row missing primary key".to_string())?;
	let mut bytes = [0u8; 16];
	bytes[..8].copy_from_slice(b"RHCL-CID");
	bytes[8..].copy_from_slice(&pk.to_be_bytes());
	Ok(Uuid::from_bytes(bytes))
}

pub async fn send_project_apply_to_cluster(
	registry: &Arc<reinhardt_cloud_grpc::registry::AgentRegistry>,
	cluster: &Cluster,
	project_name: &str,
	yaml: &str,
) -> Result<(), String> {
	validate_cluster_for_apply(cluster)?;
	let cluster_id = cluster_uuid_from_pk(cluster.id)?;
	registry
		.send_command_to_cluster(
			&cluster_id,
			AgentCommand::ApplyProject {
				project_name: project_name.to_string(),
				yaml: yaml.to_string(),
			},
		)
		.await
		.map_err(|e| {
			format!(
				"Failed to route Project apply to cluster '{}': {e}",
				cluster.name
			)
		})
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

pub async fn apply_project_yaml_for_cluster(
	yaml: &str,
	cluster: &Cluster,
) -> Result<(), String> {
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

async fn kube_client_for_namespace(namespace: &str) -> Result<KubeClient, String> {
	Ok(match KubeClient::in_cluster(namespace).await {
		Ok(client) => client,
		Err(in_cluster_error) => KubeClient::from_kubeconfig(namespace)
			.await
			.map_err(|kubeconfig_error| {
				format!(
					"Failed to build Kubernetes client from in-cluster config ({in_cluster_error}) or kubeconfig ({kubeconfig_error})"
				)
			})?,
	})
}
