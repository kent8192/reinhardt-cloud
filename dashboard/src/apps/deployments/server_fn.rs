//! Deployment server functions for the WASM dashboard.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};
use serde::{Deserialize, Serialize};

#[cfg(native)]
use reinhardt::CurrentUser;
#[cfg(wasm)]
// CurrentUser is a WASM placeholder for the `#[server_fn]` signature; native
// builds resolve the real injected user type.
#[allow(dead_code)]
struct CurrentUser<U>(pub U);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeploymentInfo {
	pub id: i64,
	pub app_name: String,
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
		app_name: deployment.app_name,
		cluster_id,
		status: deployment.status,
		image: deployment.image,
	}
}

#[cfg(native)]
fn validate_manifest(manifest: &str) -> Result<(), ServerFnError> {
	use reinhardt_cloud_types::crd::ReinhardtApp;

	if manifest.trim().is_empty() {
		return Ok(());
	}
	let app: ReinhardtApp = serde_yaml::from_str(manifest)
		.map_err(|e| ServerFnError::server(400, format!("Invalid ReinhardtApp YAML: {e}")))?;
	if let Err(errors) = app.spec.validate() {
		let messages = errors
			.into_iter()
			.map(|e| e.message)
			.collect::<Vec<_>>()
			.join("; ");
		return Err(ServerFnError::server(
			400,
			format!("Invalid ReinhardtApp spec: {messages}"),
		));
	}
	Ok(())
}

#[server_fn]
pub async fn list_deployments_for_current_org(
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> Result<Vec<DeploymentInfo>, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		let organization_id = current_org_id(&user).await?;
		let deployments = crate::apps::deployments::models::Deployment::objects()
			.filter(
				crate::apps::deployments::models::Deployment::field_organization_id()
					.eq(organization_id),
			)
			.order_by(&["id"])
			.all()
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to list deployments: {e}")))?;
		Ok(deployments.into_iter().map(deployment_info).collect())
	}
	#[cfg(wasm)]
	{
		let _ = user;
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn create_deployment_for_current_org(
	app_name: String,
	cluster_id: String,
	image: String,
	reinhardt_app_yaml: String,
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> Result<DeploymentInfo, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		let organization_id = current_org_id(&user).await?;
		let app_name = app_name.trim().to_string();
		let image = image.trim().to_string();
		let cluster_id: i64 = cluster_id
			.parse()
			.map_err(|_| ServerFnError::application("Invalid cluster_id"))?;
		if app_name.is_empty() || app_name.len() > 63 {
			return Err(ServerFnError::server(
				400,
				"App name must be 1-63 characters",
			));
		}
		if image.is_empty() || image.len() > 512 {
			return Err(ServerFnError::server(400, "Image must be 1-512 characters"));
		}
		let cluster_exists = crate::apps::clusters::models::Cluster::objects()
			.filter(crate::apps::clusters::models::Cluster::field_id().eq(cluster_id))
			.filter(
				crate::apps::clusters::models::Cluster::field_organization_id().eq(organization_id),
			)
			.exists()
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to check cluster: {e}")))?;
		if !cluster_exists {
			return Err(ServerFnError::server(404, "Cluster not found"));
		}
		validate_manifest(&reinhardt_app_yaml)?;
		let manifest = if reinhardt_app_yaml.trim().is_empty() {
			None
		} else {
			Some(reinhardt_app_yaml)
		};
		let new_deployment = crate::apps::deployments::models::Deployment::build()
			.organization(organization_id)
			.app_name(app_name)
			.cluster(cluster_id)
			.status("pending".to_string())
			.image(image)
			.reinhardt_app_yaml(manifest)
			.finish();
		let created = crate::apps::deployments::models::Deployment::objects()
			.create(&new_deployment)
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to create deployment: {e}")))?;
		Ok(deployment_info(created))
	}
	#[cfg(wasm)]
	{
		let _ = (app_name, cluster_id, image, reinhardt_app_yaml, user);
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn update_deployment_for_current_org(
	deployment_id: String,
	app_name: String,
	image: String,
	status: String,
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> Result<DeploymentInfo, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		let organization_id = current_org_id(&user).await?;
		let deployment_id: i64 = deployment_id
			.parse()
			.map_err(|_| ServerFnError::application("Invalid deployment_id"))?;
		let app_name = app_name.trim().to_string();
		let image = image.trim().to_string();
		let status = status.trim().to_string();
		if app_name.is_empty() || app_name.len() > 63 {
			return Err(ServerFnError::server(
				400,
				"App name must be 1-63 characters",
			));
		}
		if image.is_empty() || image.len() > 512 {
			return Err(ServerFnError::server(400, "Image must be 1-512 characters"));
		}
		if status.is_empty() || status.len() > 50 {
			return Err(ServerFnError::server(400, "Status must be 1-50 characters"));
		}

		let manager = crate::apps::deployments::models::Deployment::objects();
		let mut deployment = manager
			.filter(crate::apps::deployments::models::Deployment::field_id().eq(deployment_id))
			.filter(
				crate::apps::deployments::models::Deployment::field_organization_id()
					.eq(organization_id),
			)
			.first()
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to load deployment: {e}")))?
			.ok_or_else(|| ServerFnError::server(404, "Deployment not found"))?;
		deployment.app_name = app_name;
		deployment.image = image;
		deployment.status = status;
		let updated = manager
			.update(&deployment)
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to update deployment: {e}")))?;
		Ok(deployment_info(updated))
	}
	#[cfg(wasm)]
	{
		let _ = (deployment_id, app_name, image, status, user);
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn delete_deployment_for_current_org(
	deployment_id: String,
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> Result<(), ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		let organization_id = current_org_id(&user).await?;
		let deployment_id: i64 = deployment_id
			.parse()
			.map_err(|_| ServerFnError::application("Invalid deployment_id"))?;
		crate::apps::deployments::models::Deployment::objects()
			.filter(crate::apps::deployments::models::Deployment::field_id().eq(deployment_id))
			.filter(
				crate::apps::deployments::models::Deployment::field_organization_id()
					.eq(organization_id),
			)
			.first()
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to load deployment: {e}")))?
			.ok_or_else(|| ServerFnError::server(404, "Deployment not found"))?;
		crate::apps::deployments::models::Deployment::objects()
			.delete(deployment_id)
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to delete deployment: {e}")))?;
		Ok(())
	}
	#[cfg(wasm)]
	{
		let _ = (deployment_id, user);
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn update_deployment_status_for_current_org(
	deployment_id: String,
	status: String,
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> Result<DeploymentInfo, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		let organization_id = current_org_id(&user).await?;
		let deployment_id: i64 = deployment_id
			.parse()
			.map_err(|_| ServerFnError::application("Invalid deployment_id"))?;
		let status = status.trim().to_string();
		if status.is_empty() || status.len() > 50 {
			return Err(ServerFnError::server(400, "Status must be 1-50 characters"));
		}
		let manager = crate::apps::deployments::models::Deployment::objects();
		let mut deployment = manager
			.filter(crate::apps::deployments::models::Deployment::field_id().eq(deployment_id))
			.filter(
				crate::apps::deployments::models::Deployment::field_organization_id()
					.eq(organization_id),
			)
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
	#[cfg(wasm)]
	{
		let _ = (deployment_id, status, user);
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn deployment_logs_for_current_org(
	deployment_id: String,
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
	#[inject] grpc_channel: reinhardt::di::Depends<crate::config::GrpcChannelSingleton>,
) -> Result<Vec<DeploymentLogInfo>, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;
		use reinhardt_cloud_proto::common::PaginationRequest;
		use reinhardt_cloud_proto::log as log_pb;

		let organization_id = current_org_id(&user).await?;
		let deployment_id: i64 = deployment_id
			.parse()
			.map_err(|_| ServerFnError::application("Invalid deployment_id"))?;
		let deployment = crate::apps::deployments::models::Deployment::objects()
			.filter(crate::apps::deployments::models::Deployment::field_id().eq(deployment_id))
			.filter(
				crate::apps::deployments::models::Deployment::field_organization_id()
					.eq(organization_id),
			)
			.first()
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to load deployment: {e}")))?
			.ok_or_else(|| ServerFnError::server(404, "Deployment not found"))?;
		let mut client =
			log_pb::log_service_client::LogServiceClient::new(grpc_channel.channel.clone());
		let response = client
			.list_logs(log_pb::ListLogsRequest {
				filter: Some(log_pb::LogFilter {
					source: Some(deployment.app_name),
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
	#[cfg(wasm)]
	{
		let _ = (deployment_id, user, grpc_channel);
		unreachable!("server_fn body is replaced on wasm")
	}
}
