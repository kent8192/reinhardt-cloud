//! Worker Deployment builder for operator-managed background workers.

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
	Container, EnvFromSource, PodSpec, PodTemplateSpec, SecretEnvSource,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use kube::ResourceExt;
use reinhardt_cloud_types::crd::ReinhardtApp;

use super::labels::{Component, owner_reference, standard_labels};
use super::security::context::{build_container_security_context, build_pod_security_context};
use super::security::runtime_class::resolve_runtime_class_name;
use crate::error::Error;
use crate::inference::platform::Platform;

/// Builds a `Deployment` running a background worker for the given `ReinhardtApp`.
///
/// Uses the same image as the application. The default command is
/// `["manage", "run_worker"]` unless a custom command is provided.
/// Database credentials are injected from the `{app_name}-db-credentials`
/// secret via `envFrom` with `optional: true`.
pub(crate) fn build_worker_deployment(
	app: &ReinhardtApp,
	custom_command: Option<&[String]>,
	platform: &Platform,
) -> Result<Deployment, Error> {
	let labels = standard_labels(app, Component::Worker);
	let namespace = app.namespace().unwrap_or_default();
	let owner_ref = owner_reference(app)?;
	let app_name = app.name_any();
	let deploy_name = format!("{}-worker", app_name);
	let secret_name = format!("{}-db-credentials", app_name);

	let command = match custom_command {
		Some(cmd) => cmd.to_vec(),
		None => vec!["manage".to_string(), "run_worker".to_string()],
	};

	Ok(Deployment {
		metadata: ObjectMeta {
			name: Some(deploy_name),
			namespace: Some(namespace),
			labels: Some(labels.clone()),
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		spec: Some(DeploymentSpec {
			replicas: Some(1),
			selector: LabelSelector {
				match_labels: Some(BTreeMap::from([
					("app.kubernetes.io/name".to_string(), app_name.clone()),
					(
						"app.kubernetes.io/component".to_string(),
						"worker".to_string(),
					),
				])),
				..Default::default()
			},
			template: PodTemplateSpec {
				metadata: Some(ObjectMeta {
					labels: Some(labels),
					..Default::default()
				}),
				spec: Some(PodSpec {
					runtime_class_name: resolve_runtime_class_name(app, platform),
					security_context: if app.spec.isolation.is_some() {
						Some(build_pod_security_context())
					} else {
						None
					},
					containers: vec![Container {
						name: "worker".to_string(),
						image: Some(app.spec.image.clone()),
						command: Some(command),
						env_from: Some(vec![EnvFromSource {
							secret_ref: Some(SecretEnvSource {
								name: secret_name,
								optional: Some(true),
							}),
							..Default::default()
						}]),
						security_context: if app.spec.isolation.is_some() {
							Some(build_container_security_context())
						} else {
							None
						},
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
	use crate::inference::platform::Platform;
	use kube::api::ObjectMeta;
	use reinhardt_cloud_types::crd::ReinhardtAppSpec;
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
	fn test_build_worker_deployment_name() {
		// Arrange
		let app = test_app("myapp", "myapp:v1");

		// Act
		let deploy = build_worker_deployment(&app, None, &Platform::Onpremise)
			.expect("build should succeed");

		// Assert
		assert_eq!(deploy.metadata.name.as_deref(), Some("myapp-worker"));
	}

	#[rstest]
	fn test_build_worker_deployment_uses_app_image() {
		// Arrange
		let app = test_app("myapp", "registry.example.com/myapp:v2");

		// Act
		let deploy = build_worker_deployment(&app, None, &Platform::Onpremise)
			.expect("build should succeed");
		let containers = &deploy.spec.unwrap().template.spec.unwrap().containers;

		// Assert
		assert_eq!(
			containers[0].image.as_deref(),
			Some("registry.example.com/myapp:v2")
		);
	}

	#[rstest]
	fn test_build_worker_deployment_default_command() {
		// Arrange
		let app = test_app("myapp", "myapp:v1");

		// Act
		let deploy = build_worker_deployment(&app, None, &Platform::Onpremise)
			.expect("build should succeed");
		let container = &deploy.spec.unwrap().template.spec.unwrap().containers[0];

		// Assert
		assert_eq!(
			container.command.as_ref().unwrap(),
			&vec!["manage".to_string(), "run_worker".to_string()]
		);
	}

	#[rstest]
	fn test_build_worker_deployment_custom_command() {
		// Arrange
		let app = test_app("myapp", "myapp:v1");
		let custom_cmd = vec![
			"celery".to_string(),
			"worker".to_string(),
			"--pool=solo".to_string(),
		];

		// Act
		let deploy = build_worker_deployment(&app, Some(&custom_cmd), &Platform::Onpremise)
			.expect("build should succeed");
		let container = &deploy.spec.unwrap().template.spec.unwrap().containers[0];

		// Assert
		assert_eq!(
			container.command.as_ref().unwrap(),
			&vec![
				"celery".to_string(),
				"worker".to_string(),
				"--pool=solo".to_string()
			]
		);
	}

	#[rstest]
	fn test_build_worker_deployment_shares_db_credentials() {
		// Arrange
		let app = test_app("myapp", "myapp:v1");

		// Act
		let deploy = build_worker_deployment(&app, None, &Platform::Onpremise)
			.expect("build should succeed");
		let container = &deploy.spec.unwrap().template.spec.unwrap().containers[0];
		let env_from = container.env_from.as_ref().unwrap();

		// Assert
		let secret_ref = env_from[0].secret_ref.as_ref().unwrap();
		assert_eq!(secret_ref.name, "myapp-db-credentials");
		assert_eq!(secret_ref.optional, Some(true));
	}

	#[rstest]
	fn test_build_worker_deployment_component_label() {
		// Arrange
		let app = test_app("myapp", "myapp:v1");

		// Act
		let deploy = build_worker_deployment(&app, None, &Platform::Onpremise)
			.expect("build should succeed");
		let labels = deploy.metadata.labels.as_ref().unwrap();

		// Assert
		assert_eq!(labels.get("app.kubernetes.io/component").unwrap(), "worker");
	}

	#[rstest]
	fn test_worker_no_runtime_class_without_isolation() {
		// Arrange
		let app = test_app("web", "web:v1");

		// Act
		let deploy =
			build_worker_deployment(&app, None, &Platform::Aws).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert
		assert!(pod_spec.runtime_class_name.is_none());
	}

	#[rstest]
	fn test_worker_sets_runtime_class_for_microvm() {
		use reinhardt_cloud_types::crd::isolation::{IsolationLevel, IsolationSpec};

		// Arrange
		let mut app = test_app("web", "web:v1");
		app.spec.isolation = Some(IsolationSpec {
			level: IsolationLevel::MicroVM,
			..Default::default()
		});

		// Act
		let deploy =
			build_worker_deployment(&app, None, &Platform::Aws).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert
		assert_eq!(pod_spec.runtime_class_name.as_deref(), Some("kata-clh"));
	}

	#[rstest]
	fn test_worker_has_security_context_when_isolated() {
		use reinhardt_cloud_types::crd::isolation::{IsolationLevel, IsolationSpec};

		// Arrange
		let mut app = test_app("web", "web:v1");
		app.spec.isolation = Some(IsolationSpec {
			level: IsolationLevel::Sandbox,
			..Default::default()
		});

		// Act
		let deploy =
			build_worker_deployment(&app, None, &Platform::Aws).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert
		let psc = pod_spec.security_context.unwrap();
		assert_eq!(psc.run_as_non_root, Some(true));
		let container_sc = pod_spec.containers[0].security_context.as_ref().unwrap();
		assert_eq!(container_sc.allow_privilege_escalation, Some(false));
	}
}
