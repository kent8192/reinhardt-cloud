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
}
