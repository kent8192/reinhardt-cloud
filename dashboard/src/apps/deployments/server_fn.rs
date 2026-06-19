//! Deployment server functions for the WASM dashboard.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeploymentInfo {
	pub id: i64,
	pub project_name: String,
	pub cluster_id: i64,
	pub status: String,
	pub image: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeploymentLogInfo {
	pub timestamp: String,
	pub level: String,
	pub message: String,
}

#[cfg(native)]
async fn current_org_id(user: &crate::apps::auth::models::User) -> Result<i64, ServerFnError> {
	crate::apps::organizations::helpers::current_organization_id_for_user(user.id)
		.await
		.map_err(|e| ServerFnError::application(e.to_string()))
}

#[cfg(native)]
fn deployment_info(deployment: crate::apps::deployments::models::Deployment) -> DeploymentInfo {
	let cluster_id = *deployment.cluster_id();
	DeploymentInfo {
		id: deployment.id.unwrap_or_default(),
		project_name: deployment.project_name,
		cluster_id,
		status: deployment.status,
		image: deployment.image,
	}
}

#[cfg(native)]
fn validate_manifest(manifest: &str) -> Result<(), ServerFnError> {
	use reinhardt_cloud_types::crd::Project;

	if manifest.trim().is_empty() {
		return Ok(());
	}
	let app: Project = serde_yaml::from_str(manifest)
		.map_err(|e| ServerFnError::server(400, format!("Invalid Project YAML: {e}")))?;
	if let Err(errors) = app.spec.validate() {
		let messages = errors
			.into_iter()
			.map(|e| e.message)
			.collect::<Vec<_>>()
			.join("; ");
		return Err(ServerFnError::server(
			400,
			format!("Invalid Project spec: {messages}"),
		));
	}
	Ok(())
}

#[server_fn]
pub async fn list_deployments_for_current_org(
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<Vec<DeploymentInfo>, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::deployments::models::Deployment;

	let organization_id = current_org_id(&user).await?;
	let deployments = Deployment::objects()
		.filter(Deployment::field_organization_id().eq(organization_id))
		.order_by(&["id"])
		.all()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to list deployments: {e}")))?;
	Ok(deployments.into_iter().map(deployment_info).collect())
}

#[server_fn]
pub async fn create_deployment_for_current_org(
	project_name: String,
	cluster_id: String,
	image: String,
	project_yaml: String,
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<DeploymentInfo, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::clusters::models::Cluster;
	use crate::apps::deployments::models::Deployment;

	let organization_id = current_org_id(&user).await?;
	let project_name = project_name.trim().to_string();
	let image = image.trim().to_string();
	let cluster_id: i64 = cluster_id
		.parse()
		.map_err(|_| ServerFnError::application("Invalid cluster_id"))?;
	if project_name.is_empty() || project_name.len() > 63 {
		return Err(ServerFnError::server(
			400,
			"Project name must be 1-63 characters",
		));
	}
	if image.is_empty() || image.len() > 512 {
		return Err(ServerFnError::server(400, "Image must be 1-512 characters"));
	}
	let cluster_exists = Cluster::objects()
		.filter(Cluster::field_id().eq(cluster_id))
		.filter(Cluster::field_organization_id().eq(organization_id))
		.exists()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to check cluster: {e}")))?;
	if !cluster_exists {
		return Err(ServerFnError::server(404, "Cluster not found"));
	}
	validate_manifest(&project_yaml)?;
	let manifest = if project_yaml.trim().is_empty() {
		None
	} else {
		Some(project_yaml)
	};
	let new_deployment = Deployment::build()
		.organization(organization_id)
		.project_name(project_name)
		.cluster(cluster_id)
		.status("pending".to_string())
		.image(image)
		.project_yaml(manifest)
		.finish();
	let created = Deployment::objects()
		.create(&new_deployment)
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to create deployment: {e}")))?;
	Ok(deployment_info(created))
}

#[server_fn]
pub async fn update_deployment_for_current_org(
	deployment_id: String,
	project_name: String,
	image: String,
	status: String,
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<DeploymentInfo, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::deployments::models::Deployment;

	let organization_id = current_org_id(&user).await?;
	let deployment_id: i64 = deployment_id
		.parse()
		.map_err(|_| ServerFnError::application("Invalid deployment_id"))?;
	let project_name = project_name.trim().to_string();
	let image = image.trim().to_string();
	let status = status.trim().to_string();
	if project_name.is_empty() || project_name.len() > 63 {
		return Err(ServerFnError::server(
			400,
			"Project name must be 1-63 characters",
		));
	}
	if image.is_empty() || image.len() > 512 {
		return Err(ServerFnError::server(400, "Image must be 1-512 characters"));
	}
	if status.is_empty() || status.len() > 50 {
		return Err(ServerFnError::server(400, "Status must be 1-50 characters"));
	}

	let manager = Deployment::objects();
	let mut deployment = manager
		.filter(Deployment::field_id().eq(deployment_id))
		.filter(Deployment::field_organization_id().eq(organization_id))
		.first()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to load deployment: {e}")))?
		.ok_or_else(|| ServerFnError::server(404, "Deployment not found"))?;
	deployment.project_name = project_name;
	deployment.image = image;
	deployment.status = status;
	let updated = manager
		.update(&deployment)
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to update deployment: {e}")))?;
	Ok(deployment_info(updated))
}

#[server_fn]
pub async fn delete_deployment_for_current_org(
	deployment_id: String,
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<(), ServerFnError> {
	use reinhardt::Model;

	use crate::apps::deployments::models::Deployment;

	let organization_id = current_org_id(&user).await?;
	let deployment_id: i64 = deployment_id
		.parse()
		.map_err(|_| ServerFnError::application("Invalid deployment_id"))?;
	Deployment::objects()
		.filter(Deployment::field_id().eq(deployment_id))
		.filter(Deployment::field_organization_id().eq(organization_id))
		.first()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to load deployment: {e}")))?
		.ok_or_else(|| ServerFnError::server(404, "Deployment not found"))?;
	Deployment::objects()
		.delete(deployment_id)
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to delete deployment: {e}")))?;
	Ok(())
}

#[server_fn]
pub async fn update_deployment_status_for_current_org(
	deployment_id: String,
	status: String,
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<DeploymentInfo, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::deployments::models::Deployment;

	let organization_id = current_org_id(&user).await?;
	let deployment_id: i64 = deployment_id
		.parse()
		.map_err(|_| ServerFnError::application("Invalid deployment_id"))?;
	let status = status.trim().to_string();
	if status.is_empty() || status.len() > 50 {
		return Err(ServerFnError::server(400, "Status must be 1-50 characters"));
	}
	let manager = Deployment::objects();
	let mut deployment = manager
		.filter(Deployment::field_id().eq(deployment_id))
		.filter(Deployment::field_organization_id().eq(organization_id))
		.first()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to load deployment: {e}")))?
		.ok_or_else(|| ServerFnError::server(404, "Deployment not found"))?;
	deployment.status = status;
	let updated = manager
		.update(&deployment)
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to update status: {e}")))?;
	Ok(deployment_info(updated))
}

#[server_fn]
pub async fn deployment_logs_for_current_org(
	deployment_id: String,
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
	#[inject] grpc_channel: reinhardt::di::Depends<
		crate::config::GrpcChannelSingletonKey,
		crate::config::GrpcChannelSingleton,
	>,
) -> Result<Vec<DeploymentLogInfo>, ServerFnError> {
	use reinhardt::Model;
	use reinhardt_cloud_proto::common::PaginationRequest;
	use reinhardt_cloud_proto::log as log_pb;
	use reinhardt_cloud_types::crd::tenant::TenantRef;

	use crate::apps::deployments::models::Deployment;
	use crate::apps::organizations::permissions::action::Action;
	use crate::apps::organizations::permissions::guard::require_permission;

	let organization_id = require_permission(user.id, Action::LogsRead)
		.await
		.map_err(|e| ServerFnError::application(e.to_string()))?;
	let deployment_id: i64 = deployment_id
		.parse()
		.map_err(|_| ServerFnError::application("Invalid deployment_id"))?;
	let organization = crate::apps::organizations::models::Organization::objects()
		.filter(crate::apps::organizations::models::Organization::field_id().eq(organization_id))
		.first()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to load organization: {e}")))?
		.ok_or_else(|| ServerFnError::server(404, "Organization not found"))?;
	let namespace = TenantRef {
		organization: organization.slug,
		team: None,
	}
	.namespace();
	let deployment = Deployment::objects()
		.filter(Deployment::field_id().eq(deployment_id))
		.filter(Deployment::field_organization_id().eq(organization_id))
		.first()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to load deployment: {e}")))?
		.ok_or_else(|| ServerFnError::server(404, "Deployment not found"))?;
	let mut client =
		log_pb::log_service_client::LogServiceClient::new(grpc_channel.channel.clone());
	let response = client
		.list_logs(log_pb::ListLogsRequest {
			filter: Some(log_pb::LogFilter {
				source: Some(deployment.project_name),
				namespace: Some(namespace),
				..Default::default()
			}),
			pagination: Some(PaginationRequest {
				page: 1,
				page_size: 100,
			}),
		})
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to retrieve logs: {e}")))?;
	let logs = response
		.into_inner()
		.entries
		.into_iter()
		.map(|entry| {
			let timestamp = entry
				.timestamp
				.and_then(|t| {
					let nanos = if (0..=999_999_999).contains(&t.nanos) {
						t.nanos as u32
					} else {
						0
					};
					chrono::DateTime::<chrono::Utc>::from_timestamp(t.seconds, nanos)
				})
				.map(|dt| dt.to_rfc3339())
				.unwrap_or_default();
			let level = match log_pb::LogLevel::try_from(entry.level) {
				Ok(log_pb::LogLevel::Debug) => "debug",
				Ok(log_pb::LogLevel::Warn) => "warn",
				Ok(log_pb::LogLevel::Error) => "error",
				_ => "info",
			}
			.to_string();
			DeploymentLogInfo {
				timestamp,
				level,
				message: entry.message,
			}
		})
		.collect();
	Ok(logs)
}
