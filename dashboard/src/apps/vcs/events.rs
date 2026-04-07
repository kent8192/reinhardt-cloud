//! Webhook event parsing and dispatch for GitHub and GitLab.

use serde::Deserialize;

/// The action to take based on a parsed webhook event.
#[derive(Debug, Clone)]
pub enum WebhookAction {
	/// Trigger a build for the given branch and commit.
	BuildTrigger { branch: String, commit_sha: String },
	/// Create a preview environment for a pull request.
	PreviewCreate {
		pr_number: u64,
		branch: String,
		commit_sha: String,
	},
	/// Delete a preview environment for a closed pull request.
	PreviewDelete { pr_number: u64 },
	/// Release a tagged version.
	TagRelease { tag: String, commit_sha: String },
	/// Event was not actionable.
	Ignored,
}

// GitHub payload types (minimal)

#[derive(Debug, Deserialize)]
pub struct GitHubPushEvent {
	#[serde(rename = "ref")]
	pub ref_name: String,
	pub after: String,
}

#[derive(Debug, Deserialize)]
pub struct GitHubPullRequestEvent {
	pub action: String,
	pub number: u64,
	pub pull_request: GitHubPullRequest,
}

#[derive(Debug, Deserialize)]
pub struct GitHubPullRequest {
	pub head: GitHubHead,
}

#[derive(Debug, Deserialize)]
pub struct GitHubHead {
	#[serde(rename = "ref")]
	pub ref_name: String,
	pub sha: String,
}

// GitLab payload types (minimal)

#[derive(Debug, Deserialize)]
pub struct GitLabPushEvent {
	#[serde(rename = "ref")]
	pub ref_name: String,
	pub after: String,
}

#[derive(Debug, Deserialize)]
pub struct GitLabMergeRequestEvent {
	pub object_attributes: GitLabMergeRequest,
}

#[derive(Debug, Deserialize)]
pub struct GitLabMergeRequest {
	pub action: String,
	pub iid: u64,
	pub source_branch: String,
	pub last_commit: GitLabCommit,
}

#[derive(Debug, Deserialize)]
pub struct GitLabCommit {
	pub id: String,
}

/// Parses a GitLab webhook event and returns the corresponding action.
///
/// GitLab uses `X-Gitlab-Event` header values like `"Push Hook"` and
/// `"Merge Request Hook"` as event type identifiers.
pub fn parse_gitlab_event(event_type: &str, payload: &[u8]) -> WebhookAction {
	match event_type {
		"Push Hook" => {
			let event: GitLabPushEvent = match serde_json::from_slice(payload) {
				Ok(e) => e,
				Err(_) => return WebhookAction::Ignored,
			};
			if let Some(tag) = event.ref_name.strip_prefix("refs/tags/") {
				return WebhookAction::TagRelease {
					tag: tag.to_string(),
					commit_sha: event.after,
				};
			}
			let branch = event
				.ref_name
				.strip_prefix("refs/heads/")
				.unwrap_or(&event.ref_name)
				.to_string();
			WebhookAction::BuildTrigger {
				branch,
				commit_sha: event.after,
			}
		}
		"Merge Request Hook" => {
			let event: GitLabMergeRequestEvent = match serde_json::from_slice(payload) {
				Ok(e) => e,
				Err(_) => return WebhookAction::Ignored,
			};
			match event.object_attributes.action.as_str() {
				"open" | "update" | "reopen" => WebhookAction::PreviewCreate {
					pr_number: event.object_attributes.iid,
					branch: event.object_attributes.source_branch,
					commit_sha: event.object_attributes.last_commit.id,
				},
				"close" | "merge" => WebhookAction::PreviewDelete {
					pr_number: event.object_attributes.iid,
				},
				_ => WebhookAction::Ignored,
			}
		}
		_ => WebhookAction::Ignored,
	}
}

/// Parses a GitHub webhook event and returns the corresponding action.
pub fn parse_github_event(event_type: &str, payload: &[u8]) -> WebhookAction {
	match event_type {
		"push" => {
			let event: GitHubPushEvent = match serde_json::from_slice(payload) {
				Ok(e) => e,
				Err(_) => return WebhookAction::Ignored,
			};
			if let Some(tag) = event.ref_name.strip_prefix("refs/tags/") {
				return WebhookAction::TagRelease {
					tag: tag.to_string(),
					commit_sha: event.after,
				};
			}
			let branch = event
				.ref_name
				.strip_prefix("refs/heads/")
				.unwrap_or(&event.ref_name)
				.to_string();
			WebhookAction::BuildTrigger {
				branch,
				commit_sha: event.after,
			}
		}
		"pull_request" => {
			let event: GitHubPullRequestEvent = match serde_json::from_slice(payload) {
				Ok(e) => e,
				Err(_) => return WebhookAction::Ignored,
			};
			match event.action.as_str() {
				"opened" | "synchronize" | "reopened" => WebhookAction::PreviewCreate {
					pr_number: event.number,
					branch: event.pull_request.head.ref_name,
					commit_sha: event.pull_request.head.sha,
				},
				"closed" => WebhookAction::PreviewDelete {
					pr_number: event.number,
				},
				_ => WebhookAction::Ignored,
			}
		}
		_ => WebhookAction::Ignored,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_push_event_branch() {
		let payload = serde_json::json!({
			"ref": "refs/heads/main",
			"after": "abc123"
		});
		let bytes = serde_json::to_vec(&payload).unwrap();
		let action = parse_github_event("push", &bytes);
		match action {
			WebhookAction::BuildTrigger { branch, commit_sha } => {
				assert_eq!(branch, "main");
				assert_eq!(commit_sha, "abc123");
			}
			_ => panic!("Expected BuildTrigger, got {action:?}"),
		}
	}

	#[test]
	fn test_parse_push_event_tag() {
		let payload = serde_json::json!({
			"ref": "refs/tags/v1.0.0",
			"after": "def456"
		});
		let bytes = serde_json::to_vec(&payload).unwrap();
		let action = parse_github_event("push", &bytes);
		match action {
			WebhookAction::TagRelease { tag, commit_sha } => {
				assert_eq!(tag, "v1.0.0");
				assert_eq!(commit_sha, "def456");
			}
			_ => panic!("Expected TagRelease, got {action:?}"),
		}
	}

	#[test]
	fn test_parse_pr_opened() {
		let payload = serde_json::json!({
			"action": "opened",
			"number": 42,
			"pull_request": {
				"head": {
					"ref": "feature-branch",
					"sha": "sha789"
				}
			}
		});
		let bytes = serde_json::to_vec(&payload).unwrap();
		let action = parse_github_event("pull_request", &bytes);
		match action {
			WebhookAction::PreviewCreate {
				pr_number,
				branch,
				commit_sha,
			} => {
				assert_eq!(pr_number, 42);
				assert_eq!(branch, "feature-branch");
				assert_eq!(commit_sha, "sha789");
			}
			_ => panic!("Expected PreviewCreate, got {action:?}"),
		}
	}

	#[test]
	fn test_parse_pr_synchronize() {
		let payload = serde_json::json!({
			"action": "synchronize",
			"number": 42,
			"pull_request": {
				"head": {
					"ref": "feature-branch",
					"sha": "newsha"
				}
			}
		});
		let bytes = serde_json::to_vec(&payload).unwrap();
		let action = parse_github_event("pull_request", &bytes);
		assert!(matches!(action, WebhookAction::PreviewCreate { .. }));
	}

	#[test]
	fn test_parse_pr_closed() {
		let payload = serde_json::json!({
			"action": "closed",
			"number": 42,
			"pull_request": {
				"head": {
					"ref": "feature-branch",
					"sha": "sha789"
				}
			}
		});
		let bytes = serde_json::to_vec(&payload).unwrap();
		let action = parse_github_event("pull_request", &bytes);
		match action {
			WebhookAction::PreviewDelete { pr_number } => {
				assert_eq!(pr_number, 42);
			}
			_ => panic!("Expected PreviewDelete, got {action:?}"),
		}
	}

	#[test]
	fn test_unknown_event_type() {
		let action = parse_github_event("ping", b"{}");
		assert!(matches!(action, WebhookAction::Ignored));
	}

	#[test]
	fn test_invalid_payload() {
		let action = parse_github_event("push", b"not json");
		assert!(matches!(action, WebhookAction::Ignored));
	}

	// GitLab event tests

	#[test]
	fn test_parse_gitlab_push_event() {
		let payload = serde_json::json!({
			"ref": "refs/heads/main",
			"after": "gl_abc123"
		});
		let bytes = serde_json::to_vec(&payload).unwrap();
		let action = parse_gitlab_event("Push Hook", &bytes);
		match action {
			WebhookAction::BuildTrigger { branch, commit_sha } => {
				assert_eq!(branch, "main");
				assert_eq!(commit_sha, "gl_abc123");
			}
			_ => panic!("Expected BuildTrigger, got {action:?}"),
		}
	}

	#[test]
	fn test_parse_gitlab_push_tag() {
		let payload = serde_json::json!({
			"ref": "refs/tags/v2.0.0",
			"after": "gl_tag456"
		});
		let bytes = serde_json::to_vec(&payload).unwrap();
		let action = parse_gitlab_event("Push Hook", &bytes);
		match action {
			WebhookAction::TagRelease { tag, commit_sha } => {
				assert_eq!(tag, "v2.0.0");
				assert_eq!(commit_sha, "gl_tag456");
			}
			_ => panic!("Expected TagRelease, got {action:?}"),
		}
	}

	#[test]
	fn test_parse_gitlab_mr_open() {
		let payload = serde_json::json!({
			"object_attributes": {
				"action": "open",
				"iid": 99,
				"source_branch": "feature/gl-thing",
				"last_commit": { "id": "gl_sha999" }
			}
		});
		let bytes = serde_json::to_vec(&payload).unwrap();
		let action = parse_gitlab_event("Merge Request Hook", &bytes);
		match action {
			WebhookAction::PreviewCreate {
				pr_number,
				branch,
				commit_sha,
			} => {
				assert_eq!(pr_number, 99);
				assert_eq!(branch, "feature/gl-thing");
				assert_eq!(commit_sha, "gl_sha999");
			}
			_ => panic!("Expected PreviewCreate, got {action:?}"),
		}
	}

	#[test]
	fn test_parse_gitlab_mr_merge() {
		let payload = serde_json::json!({
			"object_attributes": {
				"action": "merge",
				"iid": 99,
				"source_branch": "feature/gl-thing",
				"last_commit": { "id": "gl_sha999" }
			}
		});
		let bytes = serde_json::to_vec(&payload).unwrap();
		let action = parse_gitlab_event("Merge Request Hook", &bytes);
		match action {
			WebhookAction::PreviewDelete { pr_number } => {
				assert_eq!(pr_number, 99);
			}
			_ => panic!("Expected PreviewDelete, got {action:?}"),
		}
	}

	#[test]
	fn test_parse_gitlab_mr_close() {
		let payload = serde_json::json!({
			"object_attributes": {
				"action": "close",
				"iid": 50,
				"source_branch": "fix/bug",
				"last_commit": { "id": "abc" }
			}
		});
		let bytes = serde_json::to_vec(&payload).unwrap();
		let action = parse_gitlab_event("Merge Request Hook", &bytes);
		assert!(matches!(
			action,
			WebhookAction::PreviewDelete { pr_number: 50 }
		));
	}

	#[test]
	fn test_parse_gitlab_unknown_event() {
		let action = parse_gitlab_event("Note Hook", b"{}");
		assert!(matches!(action, WebhookAction::Ignored));
	}

	#[test]
	fn test_parse_gitlab_invalid_payload() {
		let action = parse_gitlab_event("Push Hook", b"not json");
		assert!(matches!(action, WebhookAction::Ignored));
	}
}
