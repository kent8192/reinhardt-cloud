//! GitHub repository import helpers.

use reinhardt_cloud_types::crd::ReinhardtApp;
use reinhardt_cloud_types::introspect::IntrospectOutput;
use serde_json::json;

use crate::apps::github::models::GitHubRepository;
use crate::utils::vcs::events::WebhookAction;

const DEFAULT_NAMESPACE: &str = "default";
const DEFAULT_PREVIEW_TTL: &str = "72h";

#[derive(Debug, Clone)]
pub struct GitHubImportSpec {
	pub app_name: String,
	pub namespace: String,
	pub repository_url: String,
	pub branch: String,
	pub registry: String,
	pub credentials_secret: Option<String>,
	pub introspect: Option<IntrospectOutput>,
}

pub fn default_app_name(repository: &GitHubRepository) -> String {
	sanitize_app_name(&repository.name)
}

pub fn validate_app_name(name: &str) -> Result<String, String> {
	let trimmed = name.trim();
	if trimmed.is_empty() {
		return Err("App name must be 1-63 characters".to_string());
	}
	if trimmed.len() > 63 {
		return Err("App name must be 1-63 characters".to_string());
	}
	if trimmed.starts_with('-') || trimmed.ends_with('-') {
		return Err("App name must start and end with an alphanumeric character".to_string());
	}
	if !trimmed
		.bytes()
		.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
	{
		return Err(
			"App name must contain only lowercase ASCII letters, digits, and hyphens".to_string(),
		);
	}
	Ok(trimmed.to_string())
}

pub fn validate_registry(registry: &str) -> Result<String, String> {
	let trimmed = registry.trim();
	if trimmed.is_empty() || trimmed.len() > 512 {
		return Err("Registry image prefix must be 1-512 characters".to_string());
	}
	if trimmed.contains(char::is_whitespace) {
		return Err("Registry image prefix must not contain whitespace".to_string());
	}
	Ok(trimmed.trim_end_matches(':').to_string())
}

pub fn import_spec_from_repository(
	repository: &GitHubRepository,
	app_name: &str,
	registry: &str,
) -> Result<GitHubImportSpec, String> {
	let app_name = if app_name.trim().is_empty() {
		default_app_name(repository)
	} else {
		app_name.trim().to_string()
	};
	let app_name = validate_app_name(&app_name)?;
	let registry = validate_registry(registry)?;
	Ok(GitHubImportSpec {
		app_name,
		namespace: DEFAULT_NAMESPACE.to_string(),
		repository_url: format!("https://github.com/{}.git", repository.full_name),
		branch: repository.default_branch.clone(),
		registry,
		credentials_secret: None,
		introspect: None,
	})
}

pub fn enrich_import_spec(
	spec: &mut GitHubImportSpec,
	introspect: IntrospectOutput,
	credentials_secret: Option<String>,
) {
	spec.introspect = Some(introspect);
	spec.credentials_secret = credentials_secret;
}

pub fn source_reinhardt_app_yaml(spec: &GitHubImportSpec) -> Result<String, String> {
	let image = format!("{}:pending", spec.registry);
	let manifest = json!({
		"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
		"kind": "ReinhardtApp",
		"metadata": {
			"name": spec.app_name,
			"namespace": spec.namespace,
			"annotations": {
				"reinhardt.dev/build-trigger": "initial"
			}
		},
		"spec": {
			"image": image,
			"introspect": spec.introspect.clone(),
			"source": {
				"repository": spec.repository_url,
				"branch": spec.branch,
				"provider": "github",
				"credentials_secret": spec.credentials_secret,
				"build": {
					"registry": spec.registry
				},
				"webhook": {
					"enabled": true,
					"events": ["push", "pull_request"]
				},
				"preview": {
					"enabled": true,
					"ttl": DEFAULT_PREVIEW_TTL,
					"url_template": "pr-{pr_number}.{app}.preview.localhost",
					"overrides": {
						"replicas": 1,
						"database": false,
						"cache": false
					}
				}
			}
		}
	});
	let app: ReinhardtApp = serde_json::from_value(manifest)
		.map_err(|e| format!("Failed to build ReinhardtApp manifest: {e}"))?;
	if let Err(errors) = app.spec.validate() {
		let messages = errors
			.into_iter()
			.map(|e| e.message)
			.collect::<Vec<_>>()
			.join("; ");
		return Err(format!("Invalid ReinhardtApp spec: {messages}"));
	}
	serde_yaml::to_string(&app)
		.map_err(|e| format!("Failed to serialize ReinhardtApp manifest: {e}"))
}

pub fn with_build_trigger(yaml: &str, commit_sha: &str) -> Result<String, String> {
	let trigger = non_empty_commit_sha(commit_sha)?;
	with_annotations(
		yaml,
		&[("reinhardt.dev/build-trigger", trigger.as_str())],
		&[],
	)
}

pub fn with_preview_create(
	yaml: &str,
	pr_number: u64,
	branch: &str,
	commit_sha: &str,
) -> Result<String, String> {
	let branch = non_empty_branch(branch)?;
	let commit_sha = non_empty_commit_sha(commit_sha)?;
	let pr_number = pr_number.to_string();
	with_annotations(
		yaml,
		&[
			("reinhardt.dev/preview-action", "create"),
			("reinhardt.dev/pr-number", pr_number.as_str()),
			("reinhardt.dev/pr-branch", branch.as_str()),
			("reinhardt.dev/build-trigger", commit_sha.as_str()),
		],
		&[],
	)
}

pub fn with_preview_delete(yaml: &str, pr_number: u64) -> Result<String, String> {
	let pr_number = pr_number.to_string();
	with_annotations(
		yaml,
		&[
			("reinhardt.dev/preview-action", "delete"),
			("reinhardt.dev/pr-number", pr_number.as_str()),
		],
		&["reinhardt.dev/build-trigger", "reinhardt.dev/pr-branch"],
	)
}

pub fn apply_webhook_action_to_manifest(
	yaml: &str,
	action: &WebhookAction,
	production_branch: &str,
) -> Result<Option<String>, String> {
	match action {
		WebhookAction::BuildTrigger { branch, commit_sha } => {
			if branch == production_branch {
				with_build_trigger(yaml, commit_sha).map(Some)
			} else {
				Ok(None)
			}
		}
		WebhookAction::PreviewCreate {
			pr_number,
			branch,
			commit_sha,
		} => with_preview_create(yaml, *pr_number, branch, commit_sha).map(Some),
		WebhookAction::PreviewDelete { pr_number } => {
			with_preview_delete(yaml, *pr_number).map(Some)
		}
		WebhookAction::TagRelease { .. } | WebhookAction::Ignored => Ok(None),
	}
}

fn with_annotations(
	yaml: &str,
	upserts: &[(&str, &str)],
	removals: &[&str],
) -> Result<String, String> {
	let mut value: serde_yaml::Value =
		serde_yaml::from_str(yaml).map_err(|e| format!("Invalid ReinhardtApp YAML: {e}"))?;
	let annotations = value
		.as_mapping_mut()
		.and_then(|root| root.get_mut("metadata"))
		.and_then(serde_yaml::Value::as_mapping_mut)
		.ok_or_else(|| "ReinhardtApp metadata must be an object".to_string())?
		.entry(serde_yaml::Value::String("annotations".to_string()))
		.or_insert_with(|| serde_yaml::Value::Mapping(Default::default()));
	let annotations = annotations
		.as_mapping_mut()
		.ok_or_else(|| "ReinhardtApp metadata.annotations must be an object".to_string())?;
	for key in removals {
		annotations.remove(serde_yaml::Value::String((*key).to_string()));
	}
	for (key, value) in upserts {
		annotations.insert(
			serde_yaml::Value::String((*key).to_string()),
			serde_yaml::Value::String((*value).to_string()),
		);
	}
	let app: ReinhardtApp =
		serde_yaml::from_value(value).map_err(|e| format!("Invalid ReinhardtApp YAML: {e}"))?;
	serde_yaml::to_string(&app)
		.map_err(|e| format!("Failed to serialize ReinhardtApp manifest: {e}"))
}

fn non_empty_branch(branch: &str) -> Result<String, String> {
	let trimmed = branch.trim();
	if trimmed.is_empty() {
		Err("Branch must be non-empty".to_string())
	} else {
		Ok(trimmed.to_string())
	}
}

fn non_empty_commit_sha(commit_sha: &str) -> Result<String, String> {
	let trimmed = commit_sha.trim();
	if trimmed.is_empty() {
		Err("Commit SHA must be non-empty".to_string())
	} else {
		Ok(trimmed.to_string())
	}
}

fn sanitize_app_name(raw: &str) -> String {
	let mut out = String::new();
	let mut previous_hyphen = false;
	for ch in raw.chars().flat_map(char::to_lowercase) {
		let next = if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
			Some(ch)
		} else if ch == '-' || ch == '_' || ch == '.' || ch.is_whitespace() {
			Some('-')
		} else {
			None
		};
		let Some(next) = next else {
			continue;
		};
		if next == '-' {
			if out.is_empty() || previous_hyphen {
				continue;
			}
			previous_hyphen = true;
		} else {
			previous_hyphen = false;
		}
		out.push(next);
		if out.len() == 63 {
			break;
		}
	}
	while out.ends_with('-') {
		out.pop();
	}
	if out.is_empty() {
		"app".to_string()
	} else {
		out
	}
}
