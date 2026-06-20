//! Agent dispatch helpers for applying `Project` manifests.

use std::sync::Arc;

use reinhardt_cloud_types::agent::AgentCommand;
use uuid::Uuid;

use crate::apps::clusters::models::Cluster;

pub fn validate_cluster_for_apply(cluster: &Cluster) -> Result<(), String> {
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
