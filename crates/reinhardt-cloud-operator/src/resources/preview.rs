//! Preview environment helpers for PR/MR-based deployments.
//!
//! Provides functions to generate labels, names, ingress hosts, and full
//! `ReinhardtAppSpec` instances for preview environments created from
//! pull/merge requests.

use std::collections::BTreeMap;

use chrono::Utc;
use reinhardt_cloud_types::crd::{DeletionPolicy, ReinhardtApp, ReinhardtAppSpec};

use crate::error::Error;

/// Returns standard labels for a preview environment resource.
///
/// Labels include:
/// - `reinhardt.dev/preview` = `"true"`
/// - `reinhardt.dev/parent-app` = the parent application name
/// - `reinhardt.dev/pr-number` = the PR/MR number
/// - `app.kubernetes.io/managed-by` = `"reinhardt-cloud"`
pub(crate) fn preview_labels(parent_name: &str, pr_number: &str) -> BTreeMap<String, String> {
	BTreeMap::from([
		("reinhardt.dev/preview".to_string(), "true".to_string()),
		(
			"reinhardt.dev/parent-app".to_string(),
			parent_name.to_string(),
		),
		("reinhardt.dev/pr-number".to_string(), pr_number.to_string()),
		(
			"app.kubernetes.io/managed-by".to_string(),
			"reinhardt-cloud".to_string(),
		),
	])
}

/// Generates the preview application name from a parent name and PR number.
///
/// Format: `{parent}-pr-{number}` (e.g., `my-app-pr-42`).
pub(crate) fn preview_app_name(parent_name: &str, pr_number: &str) -> String {
	format!("{parent_name}-pr-{pr_number}")
}

/// Replaces template placeholders in a URL template string.
///
/// Supported placeholders: `{app}`, `{pr_number}`, `{branch}`.
pub(crate) fn preview_ingress_host(
	template: &str,
	app_name: &str,
	pr_number: &str,
	branch: &str,
) -> String {
	template
		.replace("{app}", app_name)
		.replace("{pr_number}", pr_number)
		.replace("{branch}", branch)
}

/// Builds a `ReinhardtAppSpec` for a preview environment from a parent app.
///
/// The preview spec inherits most settings from the parent but overrides:
/// - `image`: built from the parent's `source.build.registry` + the given tag
/// - `replicas`: defaults to 1, overridden by `PreviewOverrides.replicas`
/// - `database`/`cache`: from `PreviewOverrides` if set, otherwise `None`
/// - `deletion_policy`: always `Delete`
/// - `source`, `introspect`, `scale`, `storage`, `mail`: always `None`
/// - `ingress_host` on `services`: generated from `url_template` if set
pub(crate) fn build_preview_spec(
	parent: &ReinhardtApp,
	pr_number: &str,
	image_tag: &str,
) -> Result<ReinhardtAppSpec, Error> {
	let parent_spec = &parent.spec;

	// Build image from parent's registry
	let registry = parent_spec
		.source
		.as_ref()
		.and_then(|s| s.build.as_ref())
		.and_then(|b| b.registry.as_deref())
		.ok_or(Error::MissingField("source.build.registry"))?;

	let image = format!("{registry}:{image_tag}");

	// Determine preview overrides
	let preview_config = parent_spec.source.as_ref().and_then(|s| s.preview.as_ref());

	let overrides = preview_config.and_then(|p| p.overrides.as_ref());

	let replicas = overrides.and_then(|o| o.replicas).or(Some(1));

	// Database and cache from overrides
	let database = overrides.and_then(|o| o.database).and_then(|enabled| {
		if enabled {
			parent_spec.database.clone()
		} else {
			None
		}
	});

	let cache = overrides.and_then(|o| o.cache).and_then(|enabled| {
		if enabled {
			parent_spec.cache.clone()
		} else {
			None
		}
	});

	// Build ingress host from url_template if available
	let app_name = preview_app_name(&kube::ResourceExt::name_any(parent), pr_number);
	let branch = parent_spec
		.source
		.as_ref()
		.and_then(|s| s.branch.as_deref())
		.unwrap_or("main");

	let ingress_host = preview_config
		.and_then(|p| p.url_template.as_deref())
		.map(|tmpl| preview_ingress_host(tmpl, &app_name, pr_number, branch));

	// Inherit services, potentially with generated ingress_host
	let services =
		parent_spec
			.services
			.as_ref()
			.map(|s| reinhardt_cloud_types::crd::ServicesSpec {
				port: s.port,
				target_port: s.target_port,
				ingress_host: ingress_host.or_else(|| s.ingress_host.clone()),
			});

	Ok(ReinhardtAppSpec {
		image,
		replicas,
		database,
		cache,
		worker: parent_spec.worker.clone(),
		auth: parent_spec.auth.clone(),
		health: parent_spec.health.clone(),
		services,
		features: parent_spec.features.clone(),
		env: parent_spec.env.clone(),
		pages: parent_spec.pages.clone(),
		isolation: parent_spec.isolation.clone(),
		deletion_policy: DeletionPolicy::Delete,
		// Always None for preview environments
		introspect: None,
		source: None,
		scale: None,
		storage: None,
		mail: None,
		// Preview environments do not inherit plugin attachments. If a plugin
		// is desired in previews, it must be re-declared on the preview spec.
		plugins: None,
		// Inherit image-pull secrets from the parent so previews can pull
		// images from the same private registry.
		image_pull_secrets: parent_spec.image_pull_secrets.clone(),
	})
}

/// Returns `true` if the preview environment TTL has expired.
///
/// `last_activity` must be an RFC 3339 timestamp. `ttl` is a duration
/// string such as `"72h"` or `"3d"`.
pub(crate) fn is_ttl_expired(last_activity: &str, ttl: &str) -> bool {
	let Some(activity_time) = chrono::DateTime::parse_from_rfc3339(last_activity).ok() else {
		return false;
	};
	let Some(duration) = parse_duration(ttl) else {
		return false;
	};
	let now = Utc::now();
	now.signed_duration_since(activity_time) > duration
}

/// Parses a duration string like `"72h"` or `"3d"` into a `chrono::Duration`.
///
/// Returns `None` for invalid or unsupported formats.
fn parse_duration(s: &str) -> Option<chrono::Duration> {
	let s = s.trim();
	if s.is_empty() {
		return None;
	}

	let (num_str, suffix) = s.split_at(s.len() - 1);
	let value: i64 = num_str.parse().ok()?;

	match suffix {
		"h" => chrono::Duration::try_hours(value),
		"d" => chrono::Duration::try_days(value),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	fn test_app_with_preview(name: &str) -> ReinhardtApp {
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": name, "namespace": "default", "uid": "test-uid" },
			"spec": {
				"image": "myapp:latest",
				"replicas": 3,
				"services": { "port": 80, "target_port": 8080 },
				"env": { "APP_ENV": "production" },
				"source": {
					"repository": "https://github.com/org/app",
					"branch": "main",
					"provider": "github",
					"build": { "registry": "ghcr.io/org/app" },
					"preview": {
						"enabled": true,
						"ttl": "72h",
						"url_template": "pr-{pr_number}.{app}.preview.example.com",
						"overrides": { "replicas": 1 }
					}
				}
			}
		});
		serde_json::from_value(json).unwrap()
	}

	#[rstest]
	fn test_preview_app_name() {
		// Arrange & Act
		let name = preview_app_name("my-app", "42");

		// Assert
		assert_eq!(name, "my-app-pr-42");
	}

	#[rstest]
	fn test_preview_labels_returns_correct_labels() {
		// Arrange & Act
		let labels = preview_labels("my-app", "42");

		// Assert
		assert_eq!(labels.get("reinhardt.dev/preview").unwrap(), "true");
		assert_eq!(labels.get("reinhardt.dev/parent-app").unwrap(), "my-app");
		assert_eq!(labels.get("reinhardt.dev/pr-number").unwrap(), "42");
		assert_eq!(
			labels.get("app.kubernetes.io/managed-by").unwrap(),
			"reinhardt-cloud"
		);
		assert_eq!(labels.len(), 4);
	}

	#[rstest]
	fn test_preview_ingress_host_replaces_template_variables() {
		// Arrange
		let template = "pr-{pr_number}.{app}.preview.example.com";

		// Act
		let host = preview_ingress_host(template, "my-app-pr-42", "42", "feature/login");

		// Assert
		assert_eq!(host, "pr-42.my-app-pr-42.preview.example.com");
	}

	#[rstest]
	fn test_build_preview_spec_overrides_replicas() {
		// Arrange
		let parent = test_app_with_preview("my-app");

		// Act
		let spec = build_preview_spec(&parent, "42", "sha-abc123").unwrap();

		// Assert — parent has 3, preview overrides to 1
		assert_eq!(spec.replicas, Some(1));
	}

	#[rstest]
	fn test_build_preview_spec_image_from_registry() {
		// Arrange
		let parent = test_app_with_preview("my-app");

		// Act
		let spec = build_preview_spec(&parent, "42", "sha-abc123").unwrap();

		// Assert
		assert_eq!(spec.image, "ghcr.io/org/app:sha-abc123");
	}

	#[rstest]
	fn test_build_preview_spec_inherits_env() {
		// Arrange
		let parent = test_app_with_preview("my-app");

		// Act
		let spec = build_preview_spec(&parent, "42", "sha-abc123").unwrap();

		// Assert
		assert_eq!(spec.env.get("APP_ENV").unwrap(), "production");
	}

	#[rstest]
	fn test_build_preview_spec_deletion_policy_is_delete() {
		// Arrange
		let parent = test_app_with_preview("my-app");

		// Act
		let spec = build_preview_spec(&parent, "42", "sha-abc123").unwrap();

		// Assert
		assert_eq!(spec.deletion_policy, DeletionPolicy::Delete);
	}

	#[rstest]
	fn test_build_preview_spec_ingress_host_from_template() {
		// Arrange
		let parent = test_app_with_preview("my-app");

		// Act
		let spec = build_preview_spec(&parent, "42", "sha-abc123").unwrap();

		// Assert
		let host = spec
			.services
			.as_ref()
			.unwrap()
			.ingress_host
			.as_ref()
			.unwrap();
		assert_eq!(host, "pr-42.my-app-pr-42.preview.example.com");
	}

	#[rstest]
	fn test_is_ttl_expired_true_when_past_ttl() {
		// Arrange — 100 hours ago with 72h TTL
		let activity = (Utc::now() - chrono::Duration::try_hours(100).unwrap()).to_rfc3339();

		// Act
		let expired = is_ttl_expired(&activity, "72h");

		// Assert
		assert!(expired);
	}

	#[rstest]
	fn test_is_ttl_expired_false_when_within_ttl() {
		// Arrange — 1 hour ago with 72h TTL
		let activity = (Utc::now() - chrono::Duration::try_hours(1).unwrap()).to_rfc3339();

		// Act
		let expired = is_ttl_expired(&activity, "72h");

		// Assert
		assert!(!expired);
	}

	#[rstest]
	fn test_parse_duration_hours() {
		// Arrange & Act & Assert
		assert_eq!(parse_duration("72h"), chrono::Duration::try_hours(72));
	}

	#[rstest]
	fn test_parse_duration_days() {
		// Arrange & Act & Assert
		assert_eq!(parse_duration("3d"), chrono::Duration::try_days(3));
	}

	#[rstest]
	fn test_parse_duration_invalid() {
		// Arrange & Act & Assert
		assert_eq!(parse_duration("abc"), None);
		assert_eq!(parse_duration(""), None);
		assert_eq!(parse_duration("72x"), None);
	}
}
