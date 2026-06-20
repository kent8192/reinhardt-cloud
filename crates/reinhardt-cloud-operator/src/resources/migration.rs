//! Migration Job builder for operator-managed `Project` resources.

use std::collections::BTreeMap;

use k8s_openapi::api::batch::v1::{Job, JobSpec};
use k8s_openapi::api::core::v1::{Container, PodSpec, PodTemplateSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use reinhardt_cloud_types::crd::Project;

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;
use crate::inference::env_vars::build_application_env_vars;
use crate::inference::platform::Platform;

/// Label carrying the short revision identifier used by migration Jobs.
pub(crate) const MIGRATION_REVISION_LABEL: &str = "paas.reinhardt-cloud.dev/migration-revision";
/// Annotation carrying the full migration revision key for diagnostics.
pub(crate) const MIGRATION_REVISION_KEY_ANNOTATION: &str =
	"paas.reinhardt-cloud.dev/migration-revision-key";

const JOB_NAME_MAX_LEN: usize = 63;
const REVISION_ID_HEX_LEN: usize = 16;

/// Derives the migration revision key for a `Project`.
///
/// The key is intentionally based on the image reference and application
/// version currently present in the CRD. If the image reference includes a
/// digest, the digest is part of the key.
pub(crate) fn migration_revision_key(app: &Project) -> String {
	let app_version = app
		.spec
		.introspect
		.as_ref()
		.map(|introspect| introspect.app.version.trim())
		.filter(|version| !version.is_empty())
		.unwrap_or("unknown");
	format!("image={};version={app_version}", app.spec.image)
}

/// Builds a stable short identifier for the migration revision key.
pub(crate) fn migration_revision_id(revision_key: &str) -> String {
	let mut hash = 0xcbf29ce484222325u64;
	for byte in revision_key.as_bytes() {
		hash ^= u64::from(*byte);
		hash = hash.wrapping_mul(0x100000001b3);
	}
	format!("{hash:0width$x}", width = REVISION_ID_HEX_LEN)
}

/// Builds the Kubernetes Job name for a project/revision pair.
pub(crate) fn migration_job_name(app: &Project, revision_key: &str) -> String {
	let revision_id = migration_revision_id(revision_key);
	let suffix = format!("-migrate-{revision_id}");
	let max_prefix_len = JOB_NAME_MAX_LEN.saturating_sub(suffix.len());
	let mut prefix: String = app.name_any().chars().take(max_prefix_len).collect();
	prefix = prefix.trim_matches('-').to_string();
	if prefix.is_empty() {
		prefix = "project".to_string();
	}
	format!("{prefix}{suffix}")
}

/// Builds a `Job` that runs database migrations for the given `Project`.
///
/// The job uses the same image as the application and executes
/// `["manage", "migrate"]`. The environment matches the application
/// workload so migrations use the same database and settings contract.
pub(crate) fn build_migration_job(
	app: &Project,
	platform: &Platform,
	revision_key: &str,
) -> Result<Job, Error> {
	let namespace = super::require_namespace(app)?;
	let revision_id = migration_revision_id(revision_key);
	let mut labels = standard_labels(app, Component::Migration);
	labels.insert(MIGRATION_REVISION_LABEL.to_string(), revision_id.clone());
	let owner_ref = owner_reference(app)?;
	let job_name = migration_job_name(app, revision_key);
	let annotations = BTreeMap::from([(
		MIGRATION_REVISION_KEY_ANNOTATION.to_string(),
		revision_key.to_string(),
	)]);

	Ok(Job {
		metadata: ObjectMeta {
			name: Some(job_name),
			namespace: Some(namespace),
			labels: Some(labels.clone()),
			annotations: Some(annotations),
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		spec: Some(JobSpec {
			backoff_limit: Some(3),
			template: PodTemplateSpec {
				metadata: Some(ObjectMeta {
					labels: Some(labels),
					..Default::default()
				}),
				spec: Some(PodSpec {
					restart_policy: Some("Never".to_string()),
					containers: vec![Container {
						name: "migrate".to_string(),
						image: Some(app.spec.image.clone()),
						command: Some(vec!["manage".to_string(), "migrate".to_string()]),
						env: Some(build_application_env_vars(app, platform)),
						..Default::default()
					}],
					// Forward validated spec.imagePullSecrets so the migration Job can
					// pull the application image from a private registry.
					image_pull_secrets: super::validated_image_pull_secrets(app)?,
					..Default::default()
				}),
			},
			..Default::default()
		}),
		..Default::default()
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::inference::platform::Platform;
	use kube::api::ObjectMeta;
	use reinhardt_cloud_types::crd::ProjectSpec;
	use reinhardt_cloud_types::introspect::{AppMetadata, IntrospectOutput};
	use rstest::rstest;

	fn test_app(name: &str, image: &str) -> Project {
		Project {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				..Default::default()
			},
			spec: ProjectSpec {
				image: image.to_string(),
				..Default::default()
			},
			status: None,
		}
	}

	fn build_test_job(app: &Project) -> Job {
		let revision_key = migration_revision_key(app);
		build_migration_job(app, &Platform::Onpremise, &revision_key).expect("build should succeed")
	}

	fn set_app_version(app: &mut Project, version: &str) {
		app.spec.introspect = Some(IntrospectOutput {
			app: AppMetadata {
				name: app.name_any(),
				version: version.to_string(),
			},
			..Default::default()
		});
	}

	#[rstest]
	fn test_migration_revision_key_is_stable_for_same_image_and_version() {
		// Arrange
		let mut first = test_app("my-app", "registry.example.com/my-app:v1");
		let mut second = test_app("my-app", "registry.example.com/my-app:v1");
		set_app_version(&mut first, "1.2.3");
		set_app_version(&mut second, "1.2.3");

		// Act
		let first_key = migration_revision_key(&first);
		let second_key = migration_revision_key(&second);

		// Assert
		assert_eq!(first_key, second_key);
		assert_eq!(
			migration_revision_id(&first_key),
			migration_revision_id(&second_key)
		);
	}

	#[rstest]
	fn test_migration_revision_key_changes_when_image_changes() {
		// Arrange
		let mut first = test_app("my-app", "registry.example.com/my-app:v1");
		let mut second = test_app("my-app", "registry.example.com/my-app:v2");
		set_app_version(&mut first, "1.2.3");
		set_app_version(&mut second, "1.2.3");

		// Act
		let first_key = migration_revision_key(&first);
		let second_key = migration_revision_key(&second);

		// Assert
		assert_ne!(first_key, second_key);
		assert_ne!(
			migration_revision_id(&first_key),
			migration_revision_id(&second_key)
		);
	}

	#[rstest]
	fn test_migration_revision_key_changes_when_app_version_changes() {
		// Arrange
		let mut first = test_app("my-app", "registry.example.com/my-app:v1");
		let mut second = test_app("my-app", "registry.example.com/my-app:v1");
		set_app_version(&mut first, "1.2.3");
		set_app_version(&mut second, "1.2.4");

		// Act
		let first_key = migration_revision_key(&first);
		let second_key = migration_revision_key(&second);

		// Assert
		assert_ne!(first_key, second_key);
	}

	#[rstest]
	fn test_migration_revision_key_includes_image_digest() {
		// Arrange
		let app = test_app(
			"my-app",
			"registry.example.com/my-app@sha256:0123456789abcdef",
		);

		// Act
		let revision_key = migration_revision_key(&app);

		// Assert
		assert_eq!(
			revision_key,
			"image=registry.example.com/my-app@sha256:0123456789abcdef;version=unknown"
		);
	}

	#[rstest]
	fn test_build_migration_job_name() {
		// Arrange
		let app = test_app("my-app", "my-app:v1");
		let revision_key = migration_revision_key(&app);

		// Act
		let job = build_test_job(&app);

		// Assert
		let expected = format!("my-app-migrate-{}", migration_revision_id(&revision_key));
		assert_eq!(job.metadata.name.as_deref(), Some(expected.as_str()));
	}

	#[rstest]
	fn test_build_migration_job_name_is_kubernetes_safe_for_long_project_name() {
		// Arrange
		let app = test_app(
			"very-long-project-name-that-would-exceed-the-job-name-limit-with-suffix",
			"my-app:v1",
		);

		// Act
		let job = build_test_job(&app);
		let name = job.metadata.name.expect("job name should be set");

		// Assert
		assert!(name.len() <= 63);
		let (prefix, suffix) = name
			.rsplit_once("-migrate-")
			.expect("job name must include -migrate- delimiter");
		assert_eq!(suffix.len(), REVISION_ID_HEX_LEN);
		assert!(!prefix.is_empty());
	}

	#[rstest]
	fn test_build_migration_job_command() {
		// Arrange
		let app = test_app("my-app", "my-app:v1");

		// Act
		let job = build_test_job(&app);
		let container = &job.spec.unwrap().template.spec.unwrap().containers[0];

		// Assert
		assert_eq!(
			container.command.as_ref().unwrap(),
			&vec!["manage".to_string(), "migrate".to_string()]
		);
	}

	#[rstest]
	fn test_build_migration_job_uses_app_image() {
		// Arrange
		let app = test_app("my-app", "registry.example.com/my-app:v2");

		// Act
		let job = build_test_job(&app);
		let container = &job.spec.unwrap().template.spec.unwrap().containers[0];

		// Assert
		assert_eq!(
			container.image.as_deref(),
			Some("registry.example.com/my-app:v2")
		);
	}

	#[rstest]
	fn test_build_migration_job_restart_policy() {
		// Arrange
		let app = test_app("my-app", "my-app:v1");

		// Act
		let job = build_test_job(&app);
		let pod_spec = job.spec.unwrap().template.spec.unwrap();

		// Assert
		assert_eq!(pod_spec.restart_policy.as_deref(), Some("Never"));
	}

	#[rstest]
	fn test_build_migration_job_backoff_limit() {
		// Arrange
		let app = test_app("my-app", "my-app:v1");

		// Act
		let job = build_test_job(&app);

		// Assert
		assert_eq!(job.spec.unwrap().backoff_limit, Some(3));
	}

	#[rstest]
	fn test_build_migration_job_sets_revision_metadata() {
		// Arrange
		let app = test_app("my-app", "my-app:v1");
		let revision_key = migration_revision_key(&app);
		let revision_id = migration_revision_id(&revision_key);

		// Act
		let job = build_test_job(&app);

		// Assert
		let labels = job.metadata.labels.expect("labels should be set");
		let annotations = job.metadata.annotations.expect("annotations should be set");
		assert_eq!(
			labels.get(MIGRATION_REVISION_LABEL).map(String::as_str),
			Some(revision_id.as_str())
		);
		assert_eq!(
			annotations
				.get(MIGRATION_REVISION_KEY_ANNOTATION)
				.map(String::as_str),
			Some(revision_key.as_str())
		);
	}

	// ── Image pull secrets tests ───────────────────────────────────────────

	#[rstest]
	fn test_build_migration_job_image_pull_secrets_none_when_unset() {
		// Arrange
		let app = test_app("my-app", "my-app:v1");

		// Act
		let job = build_test_job(&app);
		let pod_spec = job.spec.unwrap().template.spec.unwrap();

		// Assert
		assert!(pod_spec.image_pull_secrets.is_none());
	}

	#[rstest]
	fn test_build_migration_job_image_pull_secrets_single_passthrough() {
		use k8s_openapi::api::core::v1::LocalObjectReference;

		// Arrange
		let mut app = test_app("my-app", "my-app:v1");
		app.spec.image_pull_secrets = Some(vec![LocalObjectReference {
			name: "my-app-regcred".to_string(),
		}]);

		// Act
		let job = build_test_job(&app);
		let pod_spec = job.spec.unwrap().template.spec.unwrap();

		// Assert
		let pull_secrets = pod_spec
			.image_pull_secrets
			.expect("image_pull_secrets should be set");
		assert_eq!(pull_secrets.len(), 1);
		assert_eq!(pull_secrets[0].name, "my-app-regcred");
	}

	#[rstest]
	fn test_build_migration_job_image_pull_secrets_multiple_passthrough() {
		use k8s_openapi::api::core::v1::LocalObjectReference;

		// Arrange
		let mut app = test_app("my-app", "my-app:v1");
		app.spec.image_pull_secrets = Some(vec![
			LocalObjectReference {
				name: "my-app-regcred-primary".to_string(),
			},
			LocalObjectReference {
				name: "my-app-regcred-fallback".to_string(),
			},
		]);

		// Act
		let job = build_test_job(&app);
		let pod_spec = job.spec.unwrap().template.spec.unwrap();

		// Assert
		let pull_secrets = pod_spec
			.image_pull_secrets
			.expect("image_pull_secrets should be set");
		assert_eq!(pull_secrets.len(), 2);
		assert_eq!(pull_secrets[0].name, "my-app-regcred-primary");
		assert_eq!(pull_secrets[1].name, "my-app-regcred-fallback");
	}
}
