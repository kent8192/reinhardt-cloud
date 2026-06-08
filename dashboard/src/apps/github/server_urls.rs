//! Server routes for GitHub App webhooks.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::pages::server_fn::ServerFnRequest;
use reinhardt::reinhardt_params::Body;
use reinhardt::{Response, StatusCode, post};
use serde::Serialize;
use tracing::{error, info, warn};

use crate::apps::deployments::models::Deployment;
use crate::apps::github::models::{GitHubProject, GitHubRepository};
use crate::apps::github::services::config::GitHubAppSettings;
use crate::apps::github::services::import::apply_webhook_action_to_manifest;
use crate::apps::github::services::webhook::parse_github_webhook_dispatch;
use crate::utils::vcs::events::WebhookAction;
use crate::utils::vcs::signature::verify_github_signature;

#[derive(Debug, Serialize)]
struct GitHubWebhookResponse {
	status: &'static str,
	action: &'static str,
}

#[post("/webhooks/github/", name = "github-webhook")]
pub async fn github_webhook(
	Body(payload): Body,
	#[inject] settings: Depends<GitHubAppSettings>,
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
	Deployment::objects()
		.update(&deployment)
		.await
		.map_err(|e| {
			error!("Failed to update deployment {deployment_id} from GitHub webhook: {e}");
			AppError::Internal("Failed to update deployment".to_string())
		})?;
	info!("github webhook applied {action_name} to deployment {deployment_id}");

	json_response(
		StatusCode::ACCEPTED,
		GitHubWebhookResponse {
			status: "accepted",
			action: action_name,
		},
	)
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
