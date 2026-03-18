//! Migration Job builder for operator-managed `ReinhardtApp` resources.

use k8s_openapi::api::batch::v1::{Job, JobSpec};
use k8s_openapi::api::core::v1::{
	Container, EnvFromSource, PodSpec, PodTemplateSpec, SecretEnvSource,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use nuages_types::crd::ReinhardtApp;

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;

/// Builds a `Job` that runs database migrations for the given `ReinhardtApp`.
///
/// The job uses the same image as the application and executes
/// `["manage", "migrate"]`. Database credentials are injected from the
/// `{app_name}-db-credentials` secret via `envFrom`.
pub(crate) fn build_migration_job(app: &ReinhardtApp) -> Result<Job, Error> {
	let labels = standard_labels(app, Component::Migration);
	let namespace = app.namespace().unwrap_or_default();
	let owner_ref = owner_reference(app)?;
	let app_name = app.name_any();
	let secret_name = format!("{}-db-credentials", app_name);

	Ok(Job {
		metadata: ObjectMeta {
			name: Some(format!("{}-migrate", app_name)),
			namespace: Some(namespace),
			labels: Some(labels.clone()),
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
						command: Some(vec![
							"manage".to_string(),
							"migrate".to_string(),
						]),
						env_from: Some(vec![EnvFromSource {
							secret_ref: Some(SecretEnvSource {
								name: secret_name,
								..Default::default()
							}),
							..Default::default()
						}]),
						..Default::default()
					}],
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
	use kube::api::ObjectMeta;
	use nuages_types::crd::ReinhardtAppSpec;
	use rstest::rstest;

	fn test_app(name: &str, image: &str) -> ReinhardtApp {
		ReinhardtApp {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				..Default::default()
			},
			spec: ReinhardtAppSpec {
				image: image.to_string(),
				..Default::default()
			},
			status: None,
		}
	}

	#[rstest]
	fn test_build_migration_job_name() {
		// Arrange
		let app = test_app("my-app", "my-app:v1");

		// Act
		let job = build_migration_job(&app).expect("build should succeed");

		// Assert
		assert_eq!(job.metadata.name.as_deref(), Some("my-app-migrate"));
	}

	#[rstest]
	fn test_build_migration_job_command() {
		// Arrange
		let app = test_app("my-app", "my-app:v1");

		// Act
		let job = build_migration_job(&app).expect("build should succeed");
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
		let job = build_migration_job(&app).expect("build should succeed");
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
		let job = build_migration_job(&app).expect("build should succeed");
		let pod_spec = job.spec.unwrap().template.spec.unwrap();

		// Assert
		assert_eq!(pod_spec.restart_policy.as_deref(), Some("Never"));
	}

	#[rstest]
	fn test_build_migration_job_backoff_limit() {
		// Arrange
		let app = test_app("my-app", "my-app:v1");

		// Act
		let job = build_migration_job(&app).expect("build should succeed");

		// Assert
		assert_eq!(job.spec.unwrap().backoff_limit, Some(3));
	}
}
