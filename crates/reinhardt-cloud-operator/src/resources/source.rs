//! Kaniko build Job builder for source-driven `Project` deployments.

use k8s_openapi::api::batch::v1::{Job, JobSpec};
use k8s_openapi::api::core::v1::{
	Container, EnvVar, EnvVarSource, KeyToPath, PodSpec, PodTemplateSpec, SecretKeySelector,
	SecretVolumeSource, Volume, VolumeMount,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use reinhardt_cloud_types::crd::Project;

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;

/// Returns `true` when the app has a source specification configured.
pub(crate) fn should_build_from_source(app: &Project) -> bool {
	app.spec.source.is_some()
}

/// Resolves the image reference produced by the source build.
///
/// The returned value must match the Kaniko destination used by
/// `build_kaniko_job` so the reconciled workload pulls the image that
/// the build Job pushes.
pub(crate) fn built_image_reference(app: &Project, image_tag: &str) -> Result<String, Error> {
	let source = app
		.spec
		.source
		.as_ref()
		.ok_or(Error::MissingField("spec.source"))?;
	let base = source
		.build
		.as_ref()
		.and_then(|b| b.registry.as_deref())
		.map(str::to_string)
		.unwrap_or_else(|| image_reference_without_tag(&app.spec.image).to_string());
	Ok(format!("{base}:{image_tag}"))
}

/// Returns the only credentials Secret name that a source build may mount.
///
/// Source builds run attacker-controlled repository contents, so the
/// operator must not mount arbitrary Secret names from `spec.source`.
/// Binding the mountable Secret to the `Project` name prevents a tenant
/// from using the operator as a confused deputy to read unrelated
/// same-namespace credentials.
pub(crate) fn expected_credentials_secret_name(app: &Project) -> String {
	format!("{}-git-credentials", app.name_any())
}

fn validate_credentials_secret_name(app: &Project, secret_name: &str) -> Result<(), Error> {
	let expected = expected_credentials_secret_name(app);
	if secret_name == expected {
		Ok(())
	} else {
		Err(Error::InvalidCredentialsSecret {
			actual: secret_name.to_string(),
			expected,
		})
	}
}

fn image_reference_without_tag(image: &str) -> &str {
	let image_without_digest = image.split_once('@').map_or(image, |(base, _)| base);
	let last_slash = image_without_digest.rfind('/');
	let last_colon = image_without_digest.rfind(':');

	if matches!((last_slash, last_colon), (_, Some(colon)) if last_slash.is_none_or(|slash| colon > slash))
	{
		&image_without_digest[..last_colon.expect("colon exists")]
	} else {
		image_without_digest
	}
}

/// Builds a kaniko `Job` that clones the source repository and pushes
/// the resulting container image to the configured registry.
///
/// Returns `Error::MissingField` if `spec.source` is not set.
pub(crate) fn build_kaniko_job(app: &Project, image_tag: &str) -> Result<Job, Error> {
	build_kaniko_job_for_branch(app, image_tag, None)
}

pub(crate) fn build_job_name(project_name: &str, image_tag: &str) -> String {
	let image_tag_name = image_tag
		.chars()
		.map(|ch| {
			if ch.is_ascii_alphanumeric() || ch == '-' {
				ch.to_ascii_lowercase()
			} else {
				'-'
			}
		})
		.collect::<String>()
		.trim_matches('-')
		.to_string();
	let image_tag_name = if image_tag_name.is_empty() {
		"image".to_string()
	} else {
		image_tag_name
	};
	format!("{project_name}-build-{image_tag_name}")
}

pub(crate) fn build_kaniko_job_for_branch(
	app: &Project,
	image_tag: &str,
	branch_override: Option<&str>,
) -> Result<Job, Error> {
	let source = app
		.spec
		.source
		.as_ref()
		.ok_or(Error::MissingField("spec.source"))?;

	let namespace = super::require_namespace(app)?;
	let labels = standard_labels(app, Component::Build);
	let owner_ref = owner_reference(app)?;
	let project_name = app.name_any();

	// Resolve defaults
	let branch = branch_override
		.filter(|branch| !branch.trim().is_empty())
		.unwrap_or_else(|| source.branch.as_deref().unwrap_or("main"));
	let build = source.build.as_ref();
	let dockerfile = build
		.and_then(|b| b.dockerfile.as_deref())
		.unwrap_or("./Dockerfile");
	let context = build.and_then(|b| b.context.as_deref()).unwrap_or(".");
	let destination = built_image_reference(app, image_tag)?;

	let job_name = build_job_name(&project_name, image_tag);

	// Build kaniko args
	let mut args = vec![
		format!("--git=branch={branch},url={}", source.repository),
		format!("--dockerfile={dockerfile}"),
		format!("--context=dir://{context}"),
		format!("--destination={destination}"),
		"--cache=true".to_string(),
	];

	if let Some(b) = build {
		for (key, value) in &b.build_args {
			args.push(format!("--build-arg={key}={value}"));
		}
	}

	// Container env vars and volume mounts for credentials
	let mut env = Vec::new();
	let mut volume_mounts = Vec::new();
	let mut volumes = Vec::new();

	if let Some(ref secret_name) = source.credentials_secret {
		validate_credentials_secret_name(app, secret_name)?;

		// GIT_USERNAME for kaniko git authentication (x-access-token works for GitHub and GitLab)
		env.push(EnvVar {
			name: "GIT_USERNAME".to_string(),
			value: Some("x-access-token".to_string()),
			..Default::default()
		});
		// GIT_PASSWORD from secret (kaniko consumes GIT_USERNAME + GIT_PASSWORD since v1.9.0)
		env.push(EnvVar {
			name: "GIT_PASSWORD".to_string(),
			value_from: Some(EnvVarSource {
				secret_key_ref: Some(SecretKeySelector {
					name: secret_name.clone(),
					key: "git-token".to_string(),
					optional: Some(true),
				}),
				..Default::default()
			}),
			..Default::default()
		});

		// Registry auth volume mount
		volume_mounts.push(VolumeMount {
			name: "registry-auth".to_string(),
			mount_path: "/kaniko/.docker/config.json".to_string(),
			sub_path: Some("config.json".to_string()),
			read_only: Some(true),
			..Default::default()
		});

		volumes.push(Volume {
			name: "registry-auth".to_string(),
			secret: Some(SecretVolumeSource {
				secret_name: Some(secret_name.clone()),
				items: Some(vec![KeyToPath {
					key: "registry-auth".to_string(),
					path: "config.json".to_string(),
					..Default::default()
				}]),
				optional: Some(true),
				..Default::default()
			}),
			..Default::default()
		});
	}

	Ok(Job {
		metadata: ObjectMeta {
			name: Some(job_name),
			namespace: Some(namespace),
			labels: Some(labels.clone()),
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		spec: Some(JobSpec {
			backoff_limit: Some(2),
			template: PodTemplateSpec {
				metadata: Some(ObjectMeta {
					labels: Some(labels),
					..Default::default()
				}),
				spec: Some(PodSpec {
					restart_policy: Some("Never".to_string()),
					containers: vec![Container {
						name: "kaniko".to_string(),
						image: Some("gcr.io/kaniko-project/executor:latest".to_string()),
						args: Some(args),
						env: if env.is_empty() { None } else { Some(env) },
						volume_mounts: if volume_mounts.is_empty() {
							None
						} else {
							Some(volume_mounts)
						},
						..Default::default()
					}],
					volumes: if volumes.is_empty() {
						None
					} else {
						Some(volumes)
					},
					// Forward spec.imagePullSecrets so clusters that mirror
					// the Kaniko executor image into a private registry can
					// authenticate when pulling it for the build Job.
					image_pull_secrets: app.spec.image_pull_secrets.clone(),
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
	use rstest::rstest;

	fn test_app_with_source(name: &str) -> Project {
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "Project",
			"metadata": { "name": name, "namespace": "default", "uid": "test-uid" },
			"spec": {
				"image": "placeholder:latest",
				"source": {
					"repository": "https://github.com/org/app",
					"branch": "main",
					"provider": "github",
					"credentials_secret": "my-app-git-credentials",
					"build": {
						"registry": "ghcr.io/org/app",
						"dockerfile": "./Dockerfile.prod",
						"context": "./backend"
					}
				}
			}
		});
		serde_json::from_value(json).unwrap()
	}

	fn test_app_without_source(name: &str) -> Project {
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "Project",
			"metadata": { "name": name, "namespace": "default", "uid": "test-uid" },
			"spec": {
				"image": "myapp:v1"
			}
		});
		serde_json::from_value(json).unwrap()
	}

	#[rstest]
	fn test_should_build_from_source_true() {
		// Arrange
		let app = test_app_with_source("my-app");

		// Act / Assert
		assert!(should_build_from_source(&app));
	}

	#[rstest]
	fn test_should_build_from_source_false() {
		// Arrange
		let app = test_app_without_source("my-app");

		// Act / Assert
		assert!(!should_build_from_source(&app));
	}

	#[rstest]
	fn test_job_name_format() {
		// Arrange
		let app = test_app_with_source("my-app");

		// Act
		let job = build_kaniko_job(&app, "abc12345def").unwrap();

		// Assert
		assert_eq!(
			job.metadata.name.as_deref(),
			Some("my-app-build-abc12345def")
		);
	}

	#[rstest]
	fn test_job_name_uses_full_image_tag_to_distinguish_targets() {
		// Arrange
		let app = test_app_with_source("my-app");

		// Act
		let production = build_kaniko_job(&app, "my-app-abcdef12").unwrap();
		let preview = build_kaniko_job(&app, "pr-42-abcdef12").unwrap();

		// Assert
		assert_eq!(
			production.metadata.name.as_deref(),
			Some("my-app-build-my-app-abcdef12")
		);
		assert_eq!(
			preview.metadata.name.as_deref(),
			Some("my-app-build-pr-42-abcdef12")
		);
		assert_ne!(production.metadata.name, preview.metadata.name);
	}

	#[rstest]
	fn test_job_name_sanitizes_image_tag() {
		// Act
		let name = build_job_name("my-app", "Release_2026.06.18");

		// Assert
		assert_eq!(name, "my-app-build-release-2026-06-18");
	}

	#[rstest]
	fn test_job_namespace() {
		// Arrange
		let app = test_app_with_source("my-app");

		// Act
		let job = build_kaniko_job(&app, "v1").unwrap();

		// Assert
		assert_eq!(job.metadata.namespace.as_deref(), Some("default"));
	}

	#[rstest]
	fn test_container_image_is_kaniko() {
		// Arrange
		let app = test_app_with_source("my-app");

		// Act
		let job = build_kaniko_job(&app, "v1").unwrap();
		let container = &job.spec.unwrap().template.spec.unwrap().containers[0];

		// Assert
		assert_eq!(
			container.image.as_deref(),
			Some("gcr.io/kaniko-project/executor:latest")
		);
	}

	#[rstest]
	fn test_args_contain_destination() {
		// Arrange
		let app = test_app_with_source("my-app");

		// Act
		let job = build_kaniko_job(&app, "v1").unwrap();
		let container = &job.spec.unwrap().template.spec.unwrap().containers[0];
		let args = container.args.as_ref().unwrap();

		// Assert
		assert!(
			args.iter()
				.any(|a| a.contains("--destination=ghcr.io/org/app:v1"))
		);
	}

	#[rstest]
	fn test_args_use_branch_override() {
		// Arrange
		let app = test_app_with_source("my-app");

		// Act
		let job = build_kaniko_job_for_branch(&app, "v1", Some("feature/login")).unwrap();
		let container = &job.spec.unwrap().template.spec.unwrap().containers[0];
		let args = container.args.as_ref().unwrap();

		// Assert
		assert!(
			args.iter()
				.any(|a| a.contains("--git=branch=feature/login,url="))
		);
	}

	#[rstest]
	fn test_built_image_reference_uses_build_registry() {
		// Arrange
		let app = test_app_with_source("my-app");

		// Act
		let image = built_image_reference(&app, "v1").unwrap();

		// Assert
		assert_eq!(image, "ghcr.io/org/app:v1");
	}

	#[rstest]
	fn test_built_image_reference_falls_back_to_spec_image() {
		// Arrange
		let mut app = test_app_with_source("my-app");
		app.spec.source.as_mut().unwrap().build = None;

		// Act
		let image = built_image_reference(&app, "v1").unwrap();

		// Assert
		assert_eq!(image, "placeholder:v1");
	}

	#[rstest]
	fn test_built_image_reference_fallback_preserves_registry_port() {
		// Arrange
		let mut app = test_app_with_source("my-app");
		app.spec.image = "localhost:5000/org/app:latest".to_string();
		app.spec.source.as_mut().unwrap().build = None;

		// Act
		let image = built_image_reference(&app, "v1").unwrap();

		// Assert
		assert_eq!(image, "localhost:5000/org/app:v1");
	}

	#[rstest]
	fn test_args_contain_dockerfile() {
		// Arrange
		let app = test_app_with_source("my-app");

		// Act
		let job = build_kaniko_job(&app, "v1").unwrap();
		let container = &job.spec.unwrap().template.spec.unwrap().containers[0];
		let args = container.args.as_ref().unwrap();

		// Assert
		assert!(args.iter().any(|a| a == "--dockerfile=./Dockerfile.prod"));
	}

	#[rstest]
	fn test_args_contain_context() {
		// Arrange
		let app = test_app_with_source("my-app");

		// Act
		let job = build_kaniko_job(&app, "v1").unwrap();
		let container = &job.spec.unwrap().template.spec.unwrap().containers[0];
		let args = container.args.as_ref().unwrap();

		// Assert
		assert!(args.iter().any(|a| a == "--context=dir://./backend"));
	}

	#[rstest]
	fn test_git_credentials_env_from_secret() {
		// Arrange
		let app = test_app_with_source("my-app");

		// Act
		let job = build_kaniko_job(&app, "v1").unwrap();
		let container = &job.spec.unwrap().template.spec.unwrap().containers[0];
		let env = container.env.as_ref().unwrap();

		// Assert — GIT_USERNAME is a static value for token-based auth
		let git_username = env.iter().find(|e| e.name == "GIT_USERNAME").unwrap();
		assert_eq!(git_username.value.as_deref(), Some("x-access-token"));

		// Assert — GIT_PASSWORD comes from the secret
		let git_password = env.iter().find(|e| e.name == "GIT_PASSWORD").unwrap();
		let key_ref = git_password
			.value_from
			.as_ref()
			.unwrap()
			.secret_key_ref
			.as_ref()
			.unwrap();
		assert_eq!(key_ref.name, "my-app-git-credentials");
		assert_eq!(key_ref.key, "git-token");
		assert_eq!(key_ref.optional, Some(true));
	}

	#[rstest]
	fn test_registry_auth_volume_mount() {
		// Arrange
		let app = test_app_with_source("my-app");

		// Act
		let job = build_kaniko_job(&app, "v1").unwrap();
		let pod_spec = job.spec.unwrap().template.spec.unwrap();
		let container = &pod_spec.containers[0];
		let mount = container
			.volume_mounts
			.as_ref()
			.unwrap()
			.iter()
			.find(|m| m.name == "registry-auth")
			.unwrap();

		// Assert
		assert_eq!(mount.mount_path, "/kaniko/.docker/config.json");
		assert_eq!(mount.sub_path.as_deref(), Some("config.json"));

		let volume = pod_spec
			.volumes
			.as_ref()
			.unwrap()
			.iter()
			.find(|v| v.name == "registry-auth")
			.unwrap();
		let secret = volume.secret.as_ref().unwrap();
		assert_eq!(
			secret.secret_name.as_deref(),
			Some("my-app-git-credentials")
		);
		assert_eq!(secret.optional, Some(true));
	}

	#[rstest]
	fn test_rejects_unowned_credentials_secret() {
		// Arrange
		let mut app = test_app_with_source("my-app");
		app.spec.source.as_mut().unwrap().credentials_secret =
			Some("platform-git-credentials".to_string());

		// Act
		let result = build_kaniko_job(&app, "v1");

		// Assert
		let err = result.unwrap_err();
		assert_eq!(
			err.to_string(),
			"invalid source credentials secret 'platform-git-credentials': expected 'my-app-git-credentials'"
		);
	}

	#[rstest]
	fn test_no_source_returns_error() {
		// Arrange
		let app = test_app_without_source("my-app");

		// Act
		let result = build_kaniko_job(&app, "v1");

		// Assert
		assert!(result.is_err());
		let err = result.unwrap_err();
		assert!(err.to_string().contains("spec.source"));
	}

	#[rstest]
	fn test_owner_reference_present() {
		// Arrange
		let app = test_app_with_source("my-app");

		// Act
		let job = build_kaniko_job(&app, "v1").unwrap();

		// Assert
		let owner_refs = job.metadata.owner_references.unwrap();
		assert_eq!(owner_refs.len(), 1);
		assert_eq!(owner_refs[0].name, "my-app");
	}

	// ── Image pull secrets tests ───────────────────────────────────────────

	#[rstest]
	fn test_kaniko_job_image_pull_secrets_none_when_unset() {
		// Arrange
		let app = test_app_with_source("my-app");

		// Act
		let job = build_kaniko_job(&app, "v1").unwrap();
		let pod_spec = job.spec.unwrap().template.spec.unwrap();

		// Assert
		assert!(pod_spec.image_pull_secrets.is_none());
	}

	#[rstest]
	fn test_kaniko_job_image_pull_secrets_single_passthrough() {
		use k8s_openapi::api::core::v1::LocalObjectReference;

		// Arrange
		let mut app = test_app_with_source("my-app");
		app.spec.image_pull_secrets = Some(vec![LocalObjectReference {
			name: "regcred".to_string(),
		}]);

		// Act
		let job = build_kaniko_job(&app, "v1").unwrap();
		let pod_spec = job.spec.unwrap().template.spec.unwrap();

		// Assert
		let pull_secrets = pod_spec
			.image_pull_secrets
			.expect("image_pull_secrets should be set");
		assert_eq!(pull_secrets.len(), 1);
		assert_eq!(pull_secrets[0].name, "regcred");
	}

	#[rstest]
	fn test_kaniko_job_image_pull_secrets_multiple_passthrough() {
		use k8s_openapi::api::core::v1::LocalObjectReference;

		// Arrange
		let mut app = test_app_with_source("my-app");
		app.spec.image_pull_secrets = Some(vec![
			LocalObjectReference {
				name: "regcred-primary".to_string(),
			},
			LocalObjectReference {
				name: "regcred-fallback".to_string(),
			},
		]);

		// Act
		let job = build_kaniko_job(&app, "v1").unwrap();
		let pod_spec = job.spec.unwrap().template.spec.unwrap();

		// Assert
		let pull_secrets = pod_spec
			.image_pull_secrets
			.expect("image_pull_secrets should be set");
		assert_eq!(pull_secrets.len(), 2);
		assert_eq!(pull_secrets[0].name, "regcred-primary");
		assert_eq!(pull_secrets[1].name, "regcred-fallback");
	}
}
