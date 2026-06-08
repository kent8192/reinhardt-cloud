//! GitHub webhook payload helpers.

use serde::Deserialize;

use crate::utils::vcs::events::{WebhookAction, parse_github_event};

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct GitHubWebhookDispatch {
	pub repository_id: i64,
	pub action: WebhookAction,
}

#[derive(Debug, Deserialize)]
struct GitHubRepositoryEnvelope {
	repository: GitHubRepositoryRef,
}

#[derive(Debug, Deserialize)]
struct GitHubRepositoryRef {
	id: i64,
}

pub fn parse_github_webhook_dispatch(
	event_type: &str,
	payload: &[u8],
) -> Result<GitHubWebhookDispatch, String> {
	let envelope: GitHubRepositoryEnvelope = serde_json::from_slice(payload)
		.map_err(|e| format!("GitHub webhook payload must include repository.id: {e}"))?;
	Ok(GitHubWebhookDispatch {
		repository_id: envelope.repository.id,
		action: parse_github_event(event_type, payload),
	})
}
