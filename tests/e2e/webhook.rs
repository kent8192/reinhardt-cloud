use anyhow::Result;
use reinhardt_cloud_dashboard::apps::github::services::import::apply_webhook_action_to_manifest;
use reinhardt_cloud_dashboard::apps::github::services::webhook::parse_github_webhook_dispatch;
use reinhardt_cloud_dashboard::utils::vcs::events::WebhookAction;
use reinhardt_cloud_dashboard::utils::vcs::signature::verify_github_signature;
use rstest::rstest;

#[rstest]
fn github_push_webhook_updates_production_manifest_and_ignores_other_branches() -> Result<()> {
	let yaml = source_project_yaml();
	let production_payload = serde_json::json!({
		"repository": { "id": 705 },
		"ref": "refs/heads/main",
		"after": "abcdef1234567890"
	});
	let feature_payload = serde_json::json!({
		"repository": { "id": 705 },
		"ref": "refs/heads/feature/login",
		"after": "fedcba9876543210"
	});

	let production_dispatch =
		parse_github_webhook_dispatch("push", &serde_json::to_vec(&production_payload)?)
			.map_err(anyhow::Error::msg)?;
	let feature_dispatch =
		parse_github_webhook_dispatch("push", &serde_json::to_vec(&feature_payload)?)
			.map_err(anyhow::Error::msg)?;
	let updated = apply_webhook_action_to_manifest(&yaml, &production_dispatch.action, "main")
		.map_err(anyhow::Error::msg)?
		.expect("production branch push should update the manifest");
	let ignored = apply_webhook_action_to_manifest(&yaml, &feature_dispatch.action, "main")
		.map_err(anyhow::Error::msg)?;
	let updated_value: serde_yaml::Value = serde_yaml::from_str(&updated)?;

	assert_eq!(production_dispatch.repository_id, 705);
	assert_eq!(
		production_dispatch.action,
		WebhookAction::BuildTrigger {
			branch: "main".to_string(),
			commit_sha: "abcdef1234567890".to_string(),
		}
	);
	assert_eq!(ignored, None);
	assert_eq!(
		updated_value["metadata"]["annotations"]["reinhardt.dev/build-trigger"].as_str(),
		Some("abcdef1234567890")
	);
	assert!(updated_value["metadata"]["annotations"]["reinhardt.dev/preview-action"].is_null());
	Ok(())
}

#[rstest]
fn github_pull_request_webhook_maps_preview_create_update_delete_annotations() -> Result<()> {
	let yaml = source_project_yaml();
	let opened_payload = serde_json::json!({
		"repository": { "id": 705 },
		"action": "opened",
		"number": 42,
		"pull_request": {
			"head": {
				"ref": "feature/login",
				"sha": "abc123"
			}
		}
	});
	let synchronize_payload = serde_json::json!({
		"repository": { "id": 705 },
		"action": "synchronize",
		"number": 42,
		"pull_request": {
			"head": {
				"ref": "feature/login",
				"sha": "def456"
			}
		}
	});
	let closed_payload = serde_json::json!({
		"repository": { "id": 705 },
		"action": "closed",
		"number": 42,
		"pull_request": {
			"head": {
				"ref": "feature/login",
				"sha": "def456"
			}
		}
	});

	let opened_dispatch =
		parse_github_webhook_dispatch("pull_request", &serde_json::to_vec(&opened_payload)?)
			.map_err(anyhow::Error::msg)?;
	let synchronize_dispatch =
		parse_github_webhook_dispatch("pull_request", &serde_json::to_vec(&synchronize_payload)?)
			.map_err(anyhow::Error::msg)?;
	let closed_dispatch =
		parse_github_webhook_dispatch("pull_request", &serde_json::to_vec(&closed_payload)?)
			.map_err(anyhow::Error::msg)?;

	let opened = apply_webhook_action_to_manifest(&yaml, &opened_dispatch.action, "main")
		.map_err(anyhow::Error::msg)?
		.expect("opened pull request should update the manifest");
	let synchronized =
		apply_webhook_action_to_manifest(&opened, &synchronize_dispatch.action, "main")
			.map_err(anyhow::Error::msg)?
			.expect("synchronized pull request should update the manifest");
	let closed = apply_webhook_action_to_manifest(&synchronized, &closed_dispatch.action, "main")
		.map_err(anyhow::Error::msg)?
		.expect("closed pull request should update the manifest");
	let opened_value: serde_yaml::Value = serde_yaml::from_str(&opened)?;
	let synchronized_value: serde_yaml::Value = serde_yaml::from_str(&synchronized)?;
	let closed_value: serde_yaml::Value = serde_yaml::from_str(&closed)?;

	assert_eq!(
		opened_dispatch.action,
		WebhookAction::PreviewCreate {
			pr_number: 42,
			branch: "feature/login".to_string(),
			commit_sha: "abc123".to_string(),
		}
	);
	assert_eq!(
		opened_value["metadata"]["annotations"]["reinhardt.dev/preview-action"].as_str(),
		Some("create")
	);
	assert_eq!(
		opened_value["metadata"]["annotations"]["reinhardt.dev/pr-number"].as_str(),
		Some("42")
	);
	assert_eq!(
		opened_value["metadata"]["annotations"]["reinhardt.dev/pr-branch"].as_str(),
		Some("feature/login")
	);
	assert_eq!(
		synchronized_value["metadata"]["annotations"]["reinhardt.dev/build-trigger"].as_str(),
		Some("def456")
	);
	assert_eq!(
		closed_value["metadata"]["annotations"]["reinhardt.dev/preview-action"].as_str(),
		Some("delete")
	);
	assert!(closed_value["metadata"]["annotations"]["reinhardt.dev/pr-branch"].is_null());
	assert!(closed_value["metadata"]["annotations"]["reinhardt.dev/build-trigger"].is_null());
	Ok(())
}

#[rstest]
fn github_webhook_signature_rejects_invalid_signature() {
	let payload = br#"{"repository":{"id":705}}"#;

	assert!(!verify_github_signature(
		b"webhook-secret",
		payload,
		"sha256=0000000000000000000000000000000000000000000000000000000000000000"
	));
	assert!(!verify_github_signature(
		b"webhook-secret",
		payload,
		"md5=00000000000000000000000000000000"
	));
}

fn source_project_yaml() -> String {
	serde_yaml::to_string(&serde_json::json!({
		"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
		"kind": "Project",
		"metadata": {
			"name": "webhook-app",
			"namespace": "default",
			"labels": {
				"reinhardt.dev/e2e-suite": "source-pipeline"
			}
		},
		"spec": {
			"image": "registry.local/reinhardt/webhook-app:placeholder",
			"replicas": 0,
			"source": {
				"repository": "https://github.com/kent8192/reinhardt-cloud",
				"branch": "main",
				"provider": "github",
				"build": {
					"registry": "registry.local/reinhardt/webhook-app"
				},
				"webhook": {
					"enabled": true,
					"events": ["push", "pull_request"],
					"secret_ref": "github-webhook-secret"
				},
				"preview": {
					"enabled": true,
					"ttl": "72h",
					"url_template": "pr-{pr_number}.{app}.preview.example.test",
					"overrides": {
						"replicas": 1
					}
				}
			}
		}
	}))
	.expect("source Project YAML should serialize")
}
