//! Server routes for GitHub App webhooks.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::pages::server_fn::ServerFnRequest;
use reinhardt::reinhardt_params::Body;
use reinhardt::{CurrentUser, Query, Response, StatusCode, get, post};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

use crate::apps::auth::models::User;
use crate::apps::auth::services::oauth::storage::OrmSocialAccountStorage;
use crate::apps::clusters::models::Cluster;
use crate::apps::deployments::models::Deployment;
use crate::apps::github::models::{GitHubInstallation, GitHubProject, GitHubRepository};
use crate::apps::github::services::client::{
	GitHubAppClient, GitHubUserInstallation, ReqwestGitHubAppClient,
};
use crate::apps::github::services::deploy::{
	send_git_credentials_secret_to_cluster, send_reinhardt_app_apply_to_cluster,
};
use crate::apps::github::services::import::apply_webhook_action_to_manifest;
use crate::apps::github::services::pipeline::github_credentials_secret_name;
use crate::apps::github::services::webhook::parse_github_webhook_dispatch;
use crate::apps::github::services::{GitHubAppSettings, GitHubAppSettingsKey};
use crate::apps::organizations::helpers::current_organization_id_for_user;
use crate::config::{AgentRegistrySingleton, AgentRegistrySingletonKey};
use crate::utils::vcs::events::WebhookAction;
use crate::utils::vcs::signature::verify_github_signature;

#[derive(Debug, Serialize)]
struct GitHubWebhookResponse {
	status: &'static str,
	action: &'static str,
}

#[derive(Debug, Deserialize)]
pub struct GitHubSetupQuery {
	installation_id: i64,
}

/// Complete GitHub App setup after GitHub redirects to the configured setup URL.
#[get("/setup/", name = "github-setup")]
pub async fn github_setup(
	Query(query): Query<GitHubSetupQuery>,
	#[inject] CurrentUser(user): CurrentUser<User>,
	#[inject] settings: Depends<GitHubAppSettingsKey, GitHubAppSettings>,
) -> ViewResult<Response> {
	let storage = OrmSocialAccountStorage::new();
	let Some(user_access_token) = storage
		.access_token_for_user(user.id, "github")
		.await
		.map_err(|e| {
			error!("Failed to load GitHub OAuth token for setup callback: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
	else {
		return Err(AppError::Authorization(
			"GitHub account must be linked before installing the GitHub App".to_string(),
		));
	};

	let client = ReqwestGitHubAppClient::new((*settings).clone());
	let installation = client
		.list_user_installations(&user_access_token)
		.await
		.map_err(|e| {
			error!("Failed to list GitHub installations visible to setup user: {e}");
			AppError::Internal("Failed to verify GitHub App installation".to_string())
		})?
		.into_iter()
		.find(|installation| installation.id == query.installation_id)
		.ok_or_else(|| {
			AppError::Authorization(
				"GitHub App installation is not accessible to this user".to_string(),
			)
		})?;
	let organization_id = current_organization_id_for_user(user.id).await?;
	upsert_verified_installation(organization_id, installation).await?;

	Ok(Response::temporary_redirect("/github"))
}

#[post("/webhooks/github/", name = "github-webhook")]
pub async fn github_webhook(
	Body(payload): Body,
	#[inject] settings: Depends<GitHubAppSettingsKey, GitHubAppSettings>,
	#[inject] agent_registry: Depends<AgentRegistrySingletonKey, AgentRegistrySingleton>,
	#[inject] http_request: ServerFnRequest,
) -> ViewResult<Response> {
	let event_type = required_header(&http_request, "X-GitHub-Event")?;
	let signature = required_header(&http_request, "X-Hub-Signature-256")?;
	if !verify_github_signature(settings.webhook_secret.as_bytes(), &payload, &signature) {
		warn!("github webhook rejected invalid signature for event {event_type}");
		return json_response(
			StatusCode::UNAUTHORIZED,
			GitHubWebhookResponse {
				status: "error",
				action: "invalid_signature",
			},
		);
	}
	if event_type == "installation" {
		reconcile_installation_webhook(&payload).await?;
		return json_response(
			StatusCode::ACCEPTED,
			GitHubWebhookResponse {
				status: "accepted",
				action: "installation",
			},
		);
	}

	let dispatch = parse_github_webhook_dispatch(&event_type, &payload)
		.map_err(|e| AppError::Validation(e.to_string()))?;
	let Some(repository) = GitHubRepository::objects()
		.filter(GitHubRepository::field_github_repository_id().eq(dispatch.repository_id))
		.first()
		.await
		.map_err(|e| {
			error!(
				"Failed to load GitHub repository {} for webhook: {e}",
				dispatch.repository_id
			);
			AppError::Internal("Failed to load GitHub repository".to_string())
		})?
	else {
		return json_response(
			StatusCode::OK,
			GitHubWebhookResponse {
				status: "ignored",
				action: "repository_not_imported",
			},
		);
	};
	let repository_id = repository
		.id
		.ok_or_else(|| AppError::Internal("GitHub repository missing id".to_string()))?;
	let Some(project) = GitHubProject::objects()
		.filter(GitHubProject::field_repository_id().eq(repository_id))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to load GitHub project for repository {repository_id}: {e}");
			AppError::Internal("Failed to load GitHub project".to_string())
		})?
	else {
		return json_response(
			StatusCode::OK,
			GitHubWebhookResponse {
				status: "ignored",
				action: "project_not_imported",
			},
		);
	};
	let deployment_id = *project.deployment_id();
	let mut deployment = Deployment::objects()
		.filter(Deployment::field_id().eq(deployment_id))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to load deployment {deployment_id} for webhook: {e}");
			AppError::Internal("Failed to load deployment".to_string())
		})?
		.ok_or_else(|| AppError::NotFound("Deployment not found".to_string()))?;
	let Some(current_yaml) = deployment.reinhardt_app_yaml.as_deref() else {
		return json_response(
			StatusCode::CONFLICT,
			GitHubWebhookResponse {
				status: "error",
				action: "missing_manifest",
			},
		);
	};
	let action_name = webhook_action_name(&dispatch.action);
	let Some(next_yaml) = apply_webhook_action_to_manifest(
		current_yaml,
		&dispatch.action,
		&project.production_branch,
	)
	.map_err(AppError::Validation)?
	else {
		return json_response(
			StatusCode::OK,
			GitHubWebhookResponse {
				status: "ignored",
				action: action_name,
			},
		);
	};

	deployment.reinhardt_app_yaml = Some(next_yaml);
	deployment.status = "pending".to_string();
	let manifest = deployment
		.reinhardt_app_yaml
		.as_deref()
		.ok_or_else(|| AppError::Internal("Updated deployment missing manifest".to_string()))?;
	Deployment::objects()
		.update(&deployment)
		.await
		.map_err(|e| {
			error!("Failed to update deployment {deployment_id} from GitHub webhook: {e}");
			AppError::Internal("Failed to update deployment".to_string())
		})?;
	let cluster_id = *deployment.cluster_id();
	let cluster = Cluster::objects()
		.filter(Cluster::field_id().eq(cluster_id))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to load cluster {cluster_id} for GitHub webhook: {e}");
			AppError::Internal("Failed to load deployment cluster".to_string())
		})?
		.ok_or_else(|| AppError::NotFound("Deployment cluster not found".to_string()))?;
	if let Err(e) = refresh_git_credentials_secret_for_webhook(
		&repository,
		&project,
		&cluster,
		manifest,
		&settings,
		&agent_registry,
	)
	.await
	{
		error!("Failed to refresh GitHub credentials for deployment {deployment_id}: {e}");
		deployment.status = "error".to_string();
		if let Err(update_err) = Deployment::objects().update(&deployment).await {
			error!(
				"Failed to mark deployment {deployment_id} error after GitHub credential refresh failure: {update_err}"
			);
		}
		return Err(AppError::Internal(
			"Failed to refresh GitHub repository credentials".to_string(),
		));
	}
	if let Err(e) = send_reinhardt_app_apply_to_cluster(
		&agent_registry.0,
		&cluster,
		&project.app_name,
		manifest,
	)
	.await
	{
		error!("Failed to apply deployment {deployment_id} from GitHub webhook: {e}");
		deployment.status = "error".to_string();
		if let Err(update_err) = Deployment::objects().update(&deployment).await {
			error!(
				"Failed to mark deployment {deployment_id} error after GitHub webhook apply failure: {update_err}"
			);
		}
		return Err(AppError::Internal(
			"Failed to apply ReinhardtApp manifest".to_string(),
		));
	}
	info!("github webhook applied {action_name} to deployment {deployment_id}");

	json_response(
		StatusCode::ACCEPTED,
		GitHubWebhookResponse {
			status: "accepted",
			action: action_name,
		},
	)
}

async fn upsert_verified_installation(
	organization_id: i64,
	installation: GitHubUserInstallation,
) -> Result<(), AppError> {
	let existing = GitHubInstallation::objects()
		.filter(GitHubInstallation::field_installation_id().eq(installation.id))
		.first()
		.await
		.map_err(|e| {
			error!(
				"Failed to look up GitHub installation {} during setup: {e}",
				installation.id
			);
			AppError::Internal("Failed to persist GitHub installation".to_string())
		})?;
	if let Some(mut existing) = existing {
		if *existing.organization_id() != organization_id {
			return Err(AppError::Conflict(
				"GitHub App installation is already linked to another organization".to_string(),
			));
		}
		existing.account_id = installation.account_id;
		existing.account_login = installation.account_login;
		existing.account_type = installation.account_type;
		existing.status = "active".to_string();
		GitHubInstallation::objects()
			.update(&existing)
			.await
			.map_err(|e| {
				error!(
					"Failed to update GitHub installation {} during setup: {e}",
					installation.id
				);
				AppError::Internal("Failed to persist GitHub installation".to_string())
			})?;
		return Ok(());
	}

	let row = GitHubInstallation::build()
		.organization(organization_id)
		.installation_id(installation.id)
		.account_id(installation.account_id)
		.account_login(installation.account_login)
		.account_type(installation.account_type)
		.status("active".to_string())
		.finish();
	GitHubInstallation::objects()
		.create(&row)
		.await
		.map_err(|e| {
			error!(
				"Failed to create GitHub installation {} during setup: {e}",
				installation.id
			);
			AppError::Internal("Failed to persist GitHub installation".to_string())
		})?;
	Ok(())
}

#[derive(Debug, Deserialize)]
struct GitHubInstallationWebhookPayload {
	action: String,
	installation: GitHubInstallationWebhookInstallation,
}

#[derive(Debug, Deserialize)]
struct GitHubInstallationWebhookInstallation {
	id: i64,
	account: GitHubInstallationWebhookAccount,
}

#[derive(Debug, Deserialize)]
struct GitHubInstallationWebhookAccount {
	id: i64,
	login: String,
	#[serde(rename = "type")]
	account_type: String,
}

async fn reconcile_installation_webhook(payload: &[u8]) -> Result<(), AppError> {
	let event: GitHubInstallationWebhookPayload = serde_json::from_slice(payload)
		.map_err(|e| AppError::Validation(format!("Invalid installation webhook payload: {e}")))?;
	let status = match event.action.as_str() {
		"created" | "unsuspend" | "new_permissions_accepted" => "active",
		"deleted" => "inactive",
		"suspend" => "suspended",
		_ => return Ok(()),
	};
	let Some(mut installation) = GitHubInstallation::objects()
		.filter(GitHubInstallation::field_installation_id().eq(event.installation.id))
		.first()
		.await
		.map_err(|e| {
			error!(
				"Failed to load GitHub installation {} for webhook reconciliation: {e}",
				event.installation.id
			);
			AppError::Internal("Failed to reconcile GitHub installation".to_string())
		})?
	else {
		return Ok(());
	};
	installation.account_id = event.installation.account.id;
	installation.account_login = event.installation.account.login;
	installation.account_type = event.installation.account.account_type;
	installation.status = status.to_string();
	GitHubInstallation::objects()
		.update(&installation)
		.await
		.map_err(|e| {
			error!(
				"Failed to update GitHub installation {} from webhook: {e}",
				event.installation.id
			);
			AppError::Internal("Failed to reconcile GitHub installation".to_string())
		})?;
	Ok(())
}

async fn refresh_git_credentials_secret_for_webhook(
	repository: &GitHubRepository,
	project: &GitHubProject,
	cluster: &Cluster,
	manifest: &str,
	settings: &GitHubAppSettings,
	agent_registry: &AgentRegistrySingleton,
) -> Result<(), String> {
	if !repository.private {
		return Ok(());
	}
	let installation = GitHubInstallation::objects()
		.filter(GitHubInstallation::field_id().eq(*repository.installation_id()))
		.first()
		.await
		.map_err(|e| format!("Failed to load GitHub installation: {e}"))?
		.ok_or_else(|| "GitHub installation not found".to_string())?;
	let client = ReqwestGitHubAppClient::new(settings.clone());
	let installation_token = client
		.create_installation_access_token(installation.installation_id)
		.await
		.map_err(|e| format!("Failed to mint GitHub installation access token: {e}"))?;
	let app = reinhardt_cloud_k8s::resources::parse_reinhardt_app_yaml(manifest)
		.map_err(|e| format!("Failed to parse ReinhardtApp manifest: {e}"))?;
	let namespace = app.metadata.namespace.as_deref().unwrap_or("default");
	send_git_credentials_secret_to_cluster(
		&agent_registry.0,
		cluster,
		&project.app_name,
		namespace,
		&github_credentials_secret_name(&project.app_name),
		&installation_token.token,
	)
	.await
}

fn required_header(request: &ServerFnRequest, name: &str) -> Result<String, AppError> {
	request
		.inner()
		.headers
		.get(name)
		.and_then(|value| value.to_str().ok())
		.map(str::to_string)
		.ok_or_else(|| AppError::Validation(format!("Missing required header: {name}")))
}

fn webhook_action_name(action: &WebhookAction) -> &'static str {
	match action {
		WebhookAction::BuildTrigger { .. } => "build_trigger",
		WebhookAction::PreviewCreate { .. } => "preview_create",
		WebhookAction::PreviewDelete { .. } => "preview_delete",
		WebhookAction::TagRelease { .. } => "tag_release",
		WebhookAction::Ignored => "ignored",
	}
}

fn json_response<T: Serialize>(status: StatusCode, body: T) -> ViewResult<Response> {
	let bytes = json::to_vec(&body)
		.map_err(|e| AppError::Internal(format!("Failed to serialize webhook response: {e}")))?;
	Ok(Response::new(status)
		.with_header("Content-Type", "application/json")
		.with_body(bytes))
}
