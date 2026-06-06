//! Cluster server functions for the WASM dashboard.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};
use serde::{Deserialize, Serialize};

#[cfg(native)]
use reinhardt::CurrentUser;
#[cfg(native)]
use uuid::Uuid;
#[cfg(wasm)]
#[allow(dead_code)]
struct CurrentUser<U>(pub U);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClusterInfo {
	pub id: i64,
	pub name: String,
	pub api_url: String,
	pub is_active: bool,
	pub token_last_rotated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClusterTokenInfo {
	pub cluster: ClusterInfo,
	pub auth_token: String,
}

#[cfg(native)]
async fn current_org_id(user: &crate::apps::auth::models::User) -> Result<i64, ServerFnError> {
	crate::apps::organizations::helpers::current_organization_id_for_user(user.id)
		.await
		.map_err(|e| ServerFnError::application(e.to_string()))
}

#[cfg(native)]
fn cluster_info(cluster: crate::apps::clusters::models::Cluster) -> ClusterInfo {
	ClusterInfo {
		id: cluster.id.unwrap_or_default(),
		name: cluster.name,
		api_url: cluster.api_url,
		is_active: cluster.is_active,
		token_last_rotated_at: cluster.token_last_rotated_at.map(|ts| ts.to_rfc3339()),
	}
}

#[cfg(native)]
fn cluster_id_from_pk(id: Option<i64>) -> Result<Uuid, ServerFnError> {
	let pk = id.ok_or_else(|| {
		ServerFnError::application("Cluster row missing primary key after insert")
	})?;
	let mut bytes = [0u8; 16];
	bytes[..8].copy_from_slice(b"RHCL-CID");
	bytes[8..].copy_from_slice(&pk.to_be_bytes());
	Ok(Uuid::from_bytes(bytes))
}

#[server_fn]
pub async fn list_clusters_for_current_org(
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> Result<Vec<ClusterInfo>, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		let organization_id = current_org_id(&user).await?;
		let clusters = crate::apps::clusters::models::Cluster::objects()
			.filter(
				crate::apps::clusters::models::Cluster::field_organization_id().eq(organization_id),
			)
			.order_by(&["id"])
			.all()
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to list clusters: {e}")))?;
		Ok(clusters.into_iter().map(cluster_info).collect())
	}
	#[cfg(wasm)]
	{
		let _ = user;
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn create_cluster_for_current_org(
	name: String,
	api_url: String,
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
	#[inject] agent_token_service: reinhardt::di::Depends<
		crate::apps::clusters::services::token_issuance::AgentTokenService,
	>,
) -> Result<ClusterTokenInfo, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		let organization_id = current_org_id(&user).await?;
		let name = name.trim().to_string();
		let api_url = api_url.trim().to_string();
		if name.is_empty() || name.len() > 63 {
			return Err(ServerFnError::server(
				400,
				"Cluster name must be 1-63 characters",
			));
		}
		if api_url.is_empty() || api_url.len() > 2048 {
			return Err(ServerFnError::server(
				400,
				"API URL must be 1-2048 characters",
			));
		}

		let manager = crate::apps::clusters::models::Cluster::objects();
		let new_cluster = crate::apps::clusters::models::Cluster::build()
			.organization_id(organization_id)
			.name(name)
			.api_url(api_url)
			.is_active(true)
			.token_hash(None)
			.token_last_rotated_at(None)
			.finish();
		let mut created = manager.create(&new_cluster).await.map_err(|e| {
			let msg = e.to_string();
			if msg.to_lowercase().contains("unique") || msg.to_lowercase().contains("duplicate") {
				ServerFnError::server(409, "Cluster name already exists in this organization")
			} else {
				ServerFnError::application(format!("Failed to create cluster: {msg}"))
			}
		})?;
		let cluster_uuid = cluster_id_from_pk(created.id)?;
		let issued = agent_token_service
			.issue(cluster_uuid)
			.map_err(|e| ServerFnError::application(format!("Failed to issue agent token: {e}")))?;
		created.token_hash = Some(issued.hash);
		created.token_last_rotated_at = Some(chrono::Utc::now());
		let updated = manager.update(&created).await.map_err(|e| {
			ServerFnError::application(format!("Failed to persist agent token: {e}"))
		})?;
		Ok(ClusterTokenInfo {
			cluster: cluster_info(updated),
			auth_token: issued.plaintext,
		})
	}
	#[cfg(wasm)]
	{
		let _ = (name, api_url, user, agent_token_service);
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn update_cluster_for_current_org(
	cluster_id: String,
	name: String,
	api_url: String,
	is_active: bool,
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> Result<ClusterInfo, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		let organization_id = current_org_id(&user).await?;
		let cluster_id: i64 = cluster_id
			.parse()
			.map_err(|_| ServerFnError::application("Invalid cluster_id"))?;
		let name = name.trim().to_string();
		let api_url = api_url.trim().to_string();
		if name.is_empty() || name.len() > 63 {
			return Err(ServerFnError::server(
				400,
				"Cluster name must be 1-63 characters",
			));
		}
		if api_url.is_empty() || api_url.len() > 2048 {
			return Err(ServerFnError::server(
				400,
				"API URL must be 1-2048 characters",
			));
		}

		let manager = crate::apps::clusters::models::Cluster::objects();
		let mut cluster = manager
			.filter(
				crate::apps::clusters::models::Cluster::field_organization_id().eq(organization_id),
			)
			.filter(crate::apps::clusters::models::Cluster::field_id().eq(cluster_id))
			.first()
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to load cluster: {e}")))?
			.ok_or_else(|| ServerFnError::server(404, "Cluster not found"))?;
		cluster.name = name;
		cluster.api_url = api_url;
		cluster.is_active = is_active;
		let updated = manager.update(&cluster).await.map_err(|e| {
			let msg = e.to_string();
			if msg.to_lowercase().contains("unique") || msg.to_lowercase().contains("duplicate") {
				ServerFnError::server(409, "Cluster name already exists in this organization")
			} else {
				ServerFnError::application(format!("Failed to update cluster: {msg}"))
			}
		})?;
		Ok(cluster_info(updated))
	}
	#[cfg(wasm)]
	{
		let _ = (cluster_id, name, api_url, is_active, user);
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn delete_cluster_for_current_org(
	cluster_id: String,
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> Result<(), ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		let organization_id = current_org_id(&user).await?;
		let cluster_id: i64 = cluster_id
			.parse()
			.map_err(|_| ServerFnError::application("Invalid cluster_id"))?;
		crate::apps::clusters::models::Cluster::objects()
			.filter(
				crate::apps::clusters::models::Cluster::field_organization_id().eq(organization_id),
			)
			.filter(crate::apps::clusters::models::Cluster::field_id().eq(cluster_id))
			.first()
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to load cluster: {e}")))?
			.ok_or_else(|| ServerFnError::server(404, "Cluster not found"))?;
		crate::apps::clusters::models::Cluster::objects()
			.delete(cluster_id)
			.await
			.map_err(|e| {
				let msg = e.to_string();
				if msg.to_lowercase().contains("foreign key") || msg.contains("RESTRICT") {
					ServerFnError::server(409, "Cannot delete cluster with associated deployments")
				} else {
					ServerFnError::application(format!("Failed to delete cluster: {msg}"))
				}
			})?;
		Ok(())
	}
	#[cfg(wasm)]
	{
		let _ = (cluster_id, user);
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn rotate_cluster_token_for_current_org(
	cluster_id: String,
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
	#[inject] agent_token_service: reinhardt::di::Depends<
		crate::apps::clusters::services::token_issuance::AgentTokenService,
	>,
) -> Result<ClusterTokenInfo, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		let organization_id = current_org_id(&user).await?;
		let cluster_id: i64 = cluster_id
			.parse()
			.map_err(|_| ServerFnError::application("Invalid cluster_id"))?;
		let manager = crate::apps::clusters::models::Cluster::objects();
		let mut cluster = manager
			.filter(
				crate::apps::clusters::models::Cluster::field_organization_id().eq(organization_id),
			)
			.filter(crate::apps::clusters::models::Cluster::field_id().eq(cluster_id))
			.first()
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to load cluster: {e}")))?
			.ok_or_else(|| ServerFnError::server(404, "Cluster not found"))?;
		let cluster_uuid = cluster_id_from_pk(cluster.id)?;
		let issued = agent_token_service
			.issue(cluster_uuid)
			.map_err(|e| ServerFnError::application(format!("Failed to issue agent token: {e}")))?;
		cluster.token_hash = Some(issued.hash);
		cluster.token_last_rotated_at = Some(chrono::Utc::now());
		let updated = manager.update(&cluster).await.map_err(|e| {
			ServerFnError::application(format!("Failed to persist agent token: {e}"))
		})?;
		Ok(ClusterTokenInfo {
			cluster: cluster_info(updated),
			auth_token: issued.plaintext,
		})
	}
	#[cfg(wasm)]
	{
		let _ = (cluster_id, user, agent_token_service);
		unreachable!("server_fn body is replaced on wasm")
	}
}
