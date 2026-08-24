//! Deployment server functions for the WASM dashboard.
//!
//! Organization-scoped operations resolve RBAC permissions before loading
//! deployment records so read-only members cannot perform mutations.

use reinhardt::pages::ClientForm;
use reinhardt::pages::server_fn::{ServerFnError, server_fn};
use serde::{Deserialize, Serialize};

#[cfg(native)]
use crate::utils::grpc::dashboard_grpc_auth_interceptor;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeploymentInfo {
	pub id: i64,
	pub project_name: String,
	pub cluster_id: i64,
	pub status: String,
	pub image: String,
}

/// Browser payload for creating a deployment in the current organization.
#[reinhardt::dto]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ClientForm)]
#[client_form(
	server_fn = crate::apps::deployments::server_fn::create_deployment_for_current_org,
	validate
)]
pub struct CreateDeploymentFormRequest {
	#[validate(length(min = 1, max = 63))]
	pub project_name: String,
	#[validate(length(min = 1))]
	pub cluster_id: String,
	#[validate(length(min = 1, max = 512))]
	pub image: String,
	#[validate(length(max = 65535))]
	pub project_yaml: String,
}

/// Browser payload for updating a deployment in the current organization.
#[reinhardt::dto]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ClientForm)]
#[client_form(
	server_fn = crate::apps::deployments::server_fn::update_deployment_for_current_org,
	validate
)]
pub struct UpdateDeploymentFormRequest {
	#[validate(length(min = 1))]
	pub deployment_id: String,
	#[validate(length(min = 1, max = 63))]
	pub project_name: String,
	#[validate(length(min = 1, max = 512))]
	pub image: String,
	#[validate(length(min = 1, max = 50))]
	pub status: String,
}

/// Browser payload for changing a deployment status in the current organization.
#[reinhardt::dto]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ClientForm)]
#[client_form(
	server_fn = crate::apps::deployments::server_fn::update_deployment_status_for_current_org,
	validate
)]
pub struct UpdateDeploymentStatusFormRequest {
	#[validate(length(min = 1))]
	pub deployment_id: String,
	#[validate(length(min = 1, max = 50))]
	pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProjectSourceKind {
	GitHub,
	Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreviewSummary {
	pub name: String,
	pub pr_number: String,
	pub url: Option<String>,
	pub phase: Option<String>,
	pub ready_replicas: Option<i32>,
	pub last_activity: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectPreviewSummary {
	pub deployment_id: i64,
	pub github_project_id: Option<i64>,
	pub project_name: String,
	pub display_name: String,
	pub production_branch: Option<String>,
	pub source_kind: ProjectSourceKind,
	pub previews: Vec<PreviewSummary>,
	pub preview_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeploymentLogInfo {
	pub timestamp: String,
	pub level: String,
	pub message: String,
}

#[cfg(native)]
async fn current_org_id_for_action(
	user: &crate::apps::auth::models::User,
	action: crate::apps::organizations::permissions::Action,
) -> Result<i64, ServerFnError> {
	crate::apps::organizations::permissions::require_permission(user.id, action)
		.await
		.map_err(|e| ServerFnError::application(e.to_string()))
}

#[cfg(native)]
fn deployment_info(deployment: crate::apps::deployments::models::Deployment) -> DeploymentInfo {
	let cluster_id = deployment.cluster_id();
	DeploymentInfo {
		id: deployment.id.unwrap_or_default(),
		project_name: deployment.project_name,
		cluster_id,
		status: deployment.status,
		image: deployment.image,
	}
}

#[cfg(native)]
fn dashboard_grpc_user_token(
	user: &crate::apps::auth::models::User,
	jwt_secret: &str,
) -> Result<String, ServerFnError> {
	reinhardt_cloud_core::auth::create_token(user.id, &user.username, jwt_secret.as_bytes(), 1)
		.map_err(|e| ServerFnError::application(format!("Failed to mint gRPC auth token: {e}")))
}

#[cfg(native)]
async fn preview_input_for_deployment(
	deployment: crate::apps::deployments::models::Deployment,
	organization_id: i64,
) -> Result<crate::apps::deployments::services::preview_status::PreviewProjectInput, ServerFnError>
{
	use reinhardt::Model;

	use crate::apps::deployments::services::preview_status::PreviewProjectInput;
	use crate::apps::github::models::{GitHubProject, GitHubRepository};

	let deployment_id = deployment.id.unwrap_or_default();
	let github_project = GitHubProject::objects()
		.filter(GitHubProject::field_deployment_id().eq(deployment_id))
		.filter(GitHubProject::field_organization_id().eq(organization_id))
		.first()
		.await
		.map_err(|e| {
			ServerFnError::application(format!("Failed to load GitHub project metadata: {e}"))
		})?;
	if let Some(github_project) = github_project {
		let repository_id = github_project.repository_id();
		let repository = GitHubRepository::objects()
			.filter(GitHubRepository::field_id().eq(repository_id))
			.first()
			.await
			.map_err(|e| {
				ServerFnError::application(format!(
					"Failed to load GitHub repository metadata: {e}"
				))
			})?
			.ok_or_else(|| ServerFnError::application("GitHub repository row is missing"))?;
		Ok(PreviewProjectInput {
			deployment_id,
			github_project_id: github_project.id,
			project_name: github_project.project_name,
			display_name: repository.full_name,
			production_branch: Some(github_project.production_branch),
			source_kind: ProjectSourceKind::GitHub,
		})
	} else {
		let project_name = deployment.project_name;
		Ok(PreviewProjectInput {
			deployment_id,
			github_project_id: None,
			project_name: project_name.clone(),
			display_name: project_name,
			production_branch: None,
			source_kind: ProjectSourceKind::Manual,
		})
	}
}

#[server_fn]
pub async fn list_deployments_for_current_org(
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<Vec<DeploymentInfo>, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::deployments::models::Deployment;

	let organization_id = current_org_id_for_action(
		&user,
		crate::apps::organizations::permissions::Action::DeploymentRead,
	)
	.await?;
	let deployments = Deployment::objects()
		.filter(Deployment::field_organization_id().eq(organization_id))
		.order_by(&["id"])
		.all()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to list deployments: {e}")))?;
	Ok(deployments.into_iter().map(deployment_info).collect())
}

#[server_fn]
pub async fn list_deployment_previews_for_current_org(
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<Vec<ProjectPreviewSummary>, ServerFnError> {
	let user_id = user.id;

	#[cfg(native)]
	{
		use reinhardt::Model;

		use crate::apps::deployments::models::Deployment;
		use crate::apps::deployments::services::preview_status::load_preview_summary;
		use crate::apps::organizations::permissions::action::Action;
		use crate::apps::organizations::permissions::guard::require_permission;

		let organization_id = require_permission(user_id, Action::DeploymentRead)
			.await
			.map_err(|e| ServerFnError::application(e.to_string()))?;
		let deployments = Deployment::objects()
			.filter(Deployment::field_organization_id().eq(organization_id))
			.order_by(&["id"])
			.all()
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to list deployments: {e}")))?;

		let mut summaries = Vec::with_capacity(deployments.len());
		for deployment in deployments {
			let input = preview_input_for_deployment(deployment, organization_id).await?;
			summaries.push(load_preview_summary(input, "default").await);
		}
		Ok(summaries)
	}
	#[cfg(wasm)]
	{
		let _ = user_id;
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn create_deployment_for_current_org(
	request: CreateDeploymentFormRequest,
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<DeploymentInfo, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::clusters::models::Cluster;
	use crate::apps::deployments::models::Deployment;
	use crate::apps::deployments::services::manifest::validate_project_manifest;

	let organization_id = current_org_id_for_action(
		&user,
		crate::apps::organizations::permissions::Action::DeploymentCreate,
	)
	.await?;
	reinhardt::Validate::validate(&request).map_err(ServerFnError::from)?;
	let project_name = request.project_name.trim().to_string();
	let image = request.image.trim().to_string();
	if project_name.is_empty() {
		return Err(ServerFnError::validation([(
			"project_name",
			"Project name cannot be blank",
		)]));
	}
	if image.is_empty() {
		return Err(ServerFnError::validation([(
			"image",
			"Image cannot be blank",
		)]));
	}
	let cluster_id: i64 = request
		.cluster_id
		.parse()
		.map_err(|_| ServerFnError::validation([("cluster_id", "Select a valid cluster")]))?;
	let cluster_exists = Cluster::objects()
		.filter(Cluster::field_id().eq(cluster_id))
		.filter(Cluster::field_organization_id().eq(organization_id))
		.exists()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to check cluster: {e}")))?;
	if !cluster_exists {
		return Err(ServerFnError::validation([(
			"cluster_id",
			"The selected cluster is not available",
		)]));
	}
	validate_project_manifest(&request.project_yaml)
		.map_err(|error| ServerFnError::validation([("project_yaml", error)]))?;
	let manifest = if request.project_yaml.trim().is_empty() {
		None
	} else {
		Some(request.project_yaml)
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
	request: UpdateDeploymentFormRequest,
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<DeploymentInfo, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::deployments::models::Deployment;

	let organization_id = current_org_id_for_action(
		&user,
		crate::apps::organizations::permissions::Action::DeploymentUpdate,
	)
	.await?;
	reinhardt::Validate::validate(&request).map_err(ServerFnError::from)?;
	let deployment_id: i64 = request
		.deployment_id
		.parse()
		.map_err(|_| ServerFnError::validation([("deployment_id", "Select a valid deployment")]))?;
	let project_name = request.project_name.trim().to_string();
	let image = request.image.trim().to_string();
	let status = request.status.trim().to_string();
	if project_name.is_empty() {
		return Err(ServerFnError::validation([(
			"project_name",
			"Project name cannot be blank",
		)]));
	}
	if image.is_empty() {
		return Err(ServerFnError::validation([(
			"image",
			"Image cannot be blank",
		)]));
	}
	if status.is_empty() {
		return Err(ServerFnError::validation([(
			"status",
			"Status cannot be blank",
		)]));
	}

	let manager = Deployment::objects();
	let mut deployment = manager
		.filter(Deployment::field_id().eq(deployment_id))
		.filter(Deployment::field_organization_id().eq(organization_id))
		.first()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to load deployment: {e}")))?
		.ok_or_else(|| {
			ServerFnError::validation([(
				"deployment_id",
				"The selected deployment is not available",
			)])
		})?;
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

	let organization_id = current_org_id_for_action(
		&user,
		crate::apps::organizations::permissions::Action::DeploymentDelete,
	)
	.await?;
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
	request: UpdateDeploymentStatusFormRequest,
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<DeploymentInfo, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::deployments::models::Deployment;

	let organization_id = current_org_id_for_action(
		&user,
		crate::apps::organizations::permissions::Action::DeploymentUpdate,
	)
	.await?;
	reinhardt::Validate::validate(&request).map_err(ServerFnError::from)?;
	let deployment_id: i64 = request
		.deployment_id
		.parse()
		.map_err(|_| ServerFnError::validation([("deployment_id", "Select a valid deployment")]))?;
	let status = request.status.trim().to_string();
	if status.is_empty() {
		return Err(ServerFnError::validation([(
			"status",
			"Status cannot be blank",
		)]));
	}
	let manager = Deployment::objects();
	let mut deployment = manager
		.filter(Deployment::field_id().eq(deployment_id))
		.filter(Deployment::field_organization_id().eq(organization_id))
		.first()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to load deployment: {e}")))?
		.ok_or_else(|| {
			ServerFnError::validation([(
				"deployment_id",
				"The selected deployment is not available",
			)])
		})?;
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
	#[inject] grpc_channel: reinhardt::di::KeyedDepends<
		crate::config::GrpcChannelSingletonKey,
		crate::config::GrpcChannelSingleton,
	>,
	#[inject] jwt_secret: reinhardt::di::KeyedDepends<
		crate::apps::clusters::services::JwtSecretKey,
		crate::apps::clusters::services::JwtSecret,
	>,
) -> Result<Vec<DeploymentLogInfo>, ServerFnError> {
	use reinhardt::Model;
	use reinhardt_cloud_proto::common::PaginationRequest;
	use reinhardt_cloud_proto::log as log_pb;
	use reinhardt_cloud_types::crd::tenant::TenantRef;

	use crate::apps::deployments::models::Deployment;
	use crate::apps::organizations::models::Organization;
	use crate::apps::organizations::permissions::action::Action;
	use crate::apps::organizations::permissions::guard::require_permission;

	let organization_id = require_permission(user.id, Action::LogsRead)
		.await
		.map_err(|e| ServerFnError::application(e.to_string()))?;
	let deployment_id: i64 = deployment_id
		.parse()
		.map_err(|_| ServerFnError::application("Invalid deployment_id"))?;
	let organization = Organization::objects()
		.filter(Organization::field_id().eq(organization_id))
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
	let grpc_token = dashboard_grpc_user_token(&user, &jwt_secret.0)?;
	let interceptor = dashboard_grpc_auth_interceptor(&grpc_token).map_err(|e| {
		ServerFnError::application(format!("Invalid dashboard gRPC auth metadata: {e}"))
	})?;
	let mut client = log_pb::log_service_client::LogServiceClient::with_interceptor(
		grpc_channel.channel.clone(),
		interceptor,
	);
	let response = client
		.list_logs(log_pb::ListLogsRequest {
			filter: Some(log_pb::LogFilter {
				source: Some(deployment.project_name),
				deployment_id: Some(deployment_id.to_string()),
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
