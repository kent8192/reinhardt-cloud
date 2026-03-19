//! Deployment builder for operator-managed `ReinhardtApp` resources.

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
	ConfigMapVolumeSource, Container, ContainerPort, PodSpec, PodTemplateSpec,
	ResourceRequirements, Volume, VolumeMount,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use kube::ResourceExt;
use nuages_types::crd::ReinhardtApp;

use super::labels::{Component, owner_reference, standard_labels};
use super::security::context::{build_container_security_context, build_pod_security_context};
use super::security::runtime_class::resolve_runtime_class_name;
use super::validate_port;
use crate::error::Error;
use crate::inference::env_vars::{build_system_env_vars, merge_env_vars};
use crate::inference::platform::Platform;

/// Builds a `Deployment` for the given `ReinhardtApp`.
///
/// Uses the app's own namespace as the single source of truth.
/// Returns an error if the owner reference cannot be computed.
pub(crate) fn build_deployment(app: &ReinhardtApp, platform: &Platform) -> Result<Deployment, Error> {
	let labels = standard_labels(app, Component::Web);
	let namespace = app.namespace().unwrap_or_default();
	let replicas = app.spec.replicas.unwrap_or(1);
	let port = validate_port(
		"target_port",
		app.spec
			.services
			.as_ref()
			.and_then(|s| s.target_port)
			.unwrap_or(8000),
	)?;

	let owner_ref = owner_reference(app)?;

	// Build merged environment variables (system + user overrides)
	let system_vars = build_system_env_vars();
	let merged_env = merge_env_vars(&system_vars, &app.spec.env);

	// Settings ConfigMap volume and mount
	let volumes = vec![Volume {
		name: "settings".to_string(),
		config_map: Some(ConfigMapVolumeSource {
			name: format!("{}-settings", app.name_any()),
			..Default::default()
		}),
		..Default::default()
	}];

	let volume_mounts = vec![VolumeMount {
		name: "settings".to_string(),
		mount_path: "/etc/nuages/settings".to_string(),
		read_only: Some(true),
		..Default::default()
	}];

	// Init container for database migrations when database is configured
	let init_containers = if app.spec.database.is_some() {
		Some(vec![Container {
			name: "migrate".to_string(),
			image: Some(app.spec.image.clone()),
			command: Some(vec![
				"manage".to_string(),
				"migrate".to_string(),
				"--run".to_string(),
			]),
			env: Some(merged_env.clone()),
			volume_mounts: Some(volume_mounts.clone()),
			resources: Some(ResourceRequirements {
				requests: Some(BTreeMap::from([
					("cpu".to_string(), Quantity("100m".to_string())),
					("memory".to_string(), Quantity("128Mi".to_string())),
				])),
				limits: Some(BTreeMap::from([
					("cpu".to_string(), Quantity("500m".to_string())),
					("memory".to_string(), Quantity("256Mi".to_string())),
				])),
				..Default::default()
			}),
			..Default::default()
		}])
	} else {
		None
	};

	Ok(Deployment {
		metadata: ObjectMeta {
			name: Some(app.name_any()),
			namespace: Some(namespace),
			labels: Some(labels.clone()),
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		spec: Some(DeploymentSpec {
			replicas: Some(replicas),
			selector: LabelSelector {
				match_labels: Some(BTreeMap::from([(
					"app.kubernetes.io/name".to_string(),
					app.name_any(),
				)])),
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
					init_containers,
					containers: vec![Container {
						name: app.name_any(),
						image: Some(app.spec.image.clone()),
						ports: Some(vec![ContainerPort {
							container_port: port,
							..Default::default()
						}]),
						env: Some(merged_env),
						volume_mounts: Some(volume_mounts),
						security_context: if app.spec.isolation.is_some() {
							Some(build_container_security_context())
						} else {
							None
						},
						..Default::default()
					}],
					volumes: Some(volumes),
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
	use nuages_types::crd::database::{DatabaseEngine, DatabaseSpec};
	use nuages_types::crd::isolation::{IsolationLevel, IsolationSpec};
	use nuages_types::crd::{ReinhardtAppSpec, ServicesSpec};
	use rstest::rstest;

	fn make_test_app(name: &str, image: &str, replicas: Option<i32>) -> ReinhardtApp {
		ReinhardtApp {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				..Default::default()
			},
			spec: ReinhardtAppSpec {
				image: image.to_string(),
				replicas,
				..Default::default()
			},
			status: None,
		}
	}

	fn make_test_app_with_database() -> ReinhardtApp {
		let mut app = make_test_app("web", "web:latest", None);
		app.spec.database = Some(DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: Some(20),
			version: Some("16".to_string()),
		});
		app
	}

	#[rstest]
	fn test_build_deployment_sets_image_and_replicas() {
		// Arrange
		let app = make_test_app("web", "web:latest", Some(3));

		// Act
		let deploy = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");

		// Assert
		let spec = deploy.spec.unwrap();
		assert_eq!(spec.replicas, Some(3));
		let container = &spec.template.spec.unwrap().containers[0];
		assert_eq!(container.image.as_deref(), Some("web:latest"));
	}

	#[rstest]
	fn test_build_deployment_defaults_replicas_to_one() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deploy = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");

		// Assert
		assert_eq!(deploy.spec.unwrap().replicas, Some(1));
	}

	#[rstest]
	fn test_build_deployment_defaults_port_to_8000() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deploy = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");

		// Assert
		let container = &deploy.spec.unwrap().template.spec.unwrap().containers[0];
		let port = &container.ports.as_ref().unwrap()[0];
		assert_eq!(port.container_port, 8000);
	}

	#[rstest]
	fn test_build_deployment_uses_app_namespace() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.metadata.namespace = Some("staging".to_string());

		// Act
		let deploy = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");

		// Assert
		assert_eq!(deploy.metadata.namespace.as_deref(), Some("staging"));
	}

	#[rstest]
	fn test_build_deployment_returns_error_without_uid() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.metadata.uid = None;

		// Act
		let result = build_deployment(&app, &Platform::Onpremise);

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_build_deployment_rejects_port_zero() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.services = Some(ServicesSpec {
			port: None,
			target_port: Some(0),
			ingress_host: None,
		});

		// Act
		let result = build_deployment(&app, &Platform::Onpremise);

		// Assert
		assert!(result.is_err());
		let err = result.unwrap_err().to_string();
		assert_eq!(
			err,
			"invalid port 0 for field 'target_port': must be between 1 and 65535"
		);
	}

	#[rstest]
	fn test_build_deployment_rejects_port_above_65535() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.services = Some(ServicesSpec {
			port: None,
			target_port: Some(65536),
			ingress_host: None,
		});

		// Act
		let result = build_deployment(&app, &Platform::Onpremise);

		// Assert
		assert!(result.is_err());
		let err = result.unwrap_err().to_string();
		assert_eq!(
			err,
			"invalid port 65536 for field 'target_port': must be between 1 and 65535"
		);
	}

	#[rstest]
	fn test_build_deployment_rejects_negative_port() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.services = Some(ServicesSpec {
			port: None,
			target_port: Some(-1),
			ingress_host: None,
		});

		// Act
		let result = build_deployment(&app, &Platform::Onpremise);

		// Assert
		assert!(result.is_err());
		let err = result.unwrap_err().to_string();
		assert_eq!(
			err,
			"invalid port -1 for field 'target_port': must be between 1 and 65535"
		);
	}

	#[rstest]
	fn test_build_deployment_includes_init_container_when_database() {
		// Arrange
		let app = make_test_app_with_database();

		// Act
		let deployment = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deployment.spec.unwrap().template.spec.unwrap();

		// Assert
		let init_containers = pod_spec.init_containers.unwrap();
		assert_eq!(init_containers.len(), 1);
		assert_eq!(init_containers[0].name, "migrate");
		assert_eq!(
			init_containers[0].command.as_ref().unwrap().last().unwrap(),
			"--run"
		);
	}

	#[rstest]
	fn test_build_deployment_no_init_container_without_database() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deployment = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deployment.spec.unwrap().template.spec.unwrap();

		// Assert
		assert!(pod_spec.init_containers.is_none() || pod_spec.init_containers.unwrap().is_empty());
	}

	#[rstest]
	fn test_build_deployment_has_settings_volume() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deployment = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deployment.spec.unwrap().template.spec.unwrap();

		// Assert
		let volumes = pod_spec.volumes.unwrap();
		assert!(volumes.iter().any(|v| v.name == "settings"));
		let settings_vol = volumes.iter().find(|v| v.name == "settings").unwrap();
		assert_eq!(
			settings_vol.config_map.as_ref().unwrap().name.as_str(),
			"web-settings"
		);
	}

	#[rstest]
	fn test_build_deployment_has_settings_volume_mount() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deployment = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");
		let container = &deployment.spec.unwrap().template.spec.unwrap().containers[0];

		// Assert
		let mounts = container.volume_mounts.as_ref().unwrap();
		assert!(mounts.iter().any(|m| m.name == "settings"
			&& m.mount_path == "/etc/nuages/settings"
			&& m.read_only == Some(true)));
	}

	#[rstest]
	fn test_build_deployment_injects_system_env_vars() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deployment = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");
		let containers = deployment.spec.unwrap().template.spec.unwrap().containers;
		let env = containers[0].env.as_ref().unwrap();

		// Assert
		assert!(
			env.iter()
				.any(|e| e.name == "REINHARDT_ENV" && e.value.as_deref() == Some("production"))
		);
		assert!(env.iter().any(|e| e.name == "NUAGES_CONFIG_DIR"));
	}

	#[rstest]
	fn test_build_deployment_user_env_overrides_system() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.env = BTreeMap::from([("REINHARDT_ENV".to_string(), "staging".to_string())]);

		// Act
		let deployment = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");
		let containers = deployment.spec.unwrap().template.spec.unwrap().containers;
		let env = containers[0].env.as_ref().unwrap();

		// Assert
		let reinhardt_env = env.iter().find(|e| e.name == "REINHARDT_ENV").unwrap();
		assert_eq!(reinhardt_env.value.as_deref(), Some("staging"));
	}

	#[rstest]
	fn test_build_deployment_init_container_has_same_image_as_main() {
		// Arrange
		let app = make_test_app_with_database();

		// Act
		let deployment = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deployment.spec.unwrap().template.spec.unwrap();

		// Assert
		let main_image = pod_spec.containers[0].image.as_deref();
		let init_image = pod_spec.init_containers.as_ref().unwrap()[0]
			.image
			.as_deref();
		assert_eq!(main_image, init_image);
		assert_eq!(main_image, Some("web:latest"));
	}

	#[rstest]
	fn test_build_deployment_init_container_has_resource_limits() {
		// Arrange
		let app = make_test_app_with_database();

		// Act
		let deployment = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deployment.spec.unwrap().template.spec.unwrap();
		let init_container = &pod_spec.init_containers.as_ref().unwrap()[0];

		// Assert
		let resources = init_container.resources.as_ref().unwrap();
		assert!(resources.requests.is_some());
		assert!(resources.limits.is_some());
		let requests = resources.requests.as_ref().unwrap();
		assert!(requests.contains_key("cpu"));
		assert!(requests.contains_key("memory"));
		let limits = resources.limits.as_ref().unwrap();
		assert!(limits.contains_key("cpu"));
		assert!(limits.contains_key("memory"));
	}

	#[rstest]
	fn test_build_deployment_volume_mount_path_is_settings() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deployment = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");
		let container = &deployment.spec.unwrap().template.spec.unwrap().containers[0];
		let mounts = container.volume_mounts.as_ref().unwrap();

		// Assert
		let settings_mount = mounts.iter().find(|m| m.name == "settings").unwrap();
		assert_eq!(settings_mount.mount_path, "/etc/nuages/settings");
	}

	#[rstest]
	fn test_build_deployment_user_env_var_overrides_reinhardt_env() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec
			.env
			.insert("REINHARDT_ENV".to_string(), "development".to_string());

		// Act
		let deployment = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");
		let containers = deployment.spec.unwrap().template.spec.unwrap().containers;
		let env = containers[0].env.as_ref().unwrap();

		// Assert
		let reinhardt_env = env.iter().find(|e| e.name == "REINHARDT_ENV").unwrap();
		assert_eq!(reinhardt_env.value.as_deref(), Some("development"));
		// Verify no duplicates
		let count = env.iter().filter(|e| e.name == "REINHARDT_ENV").count();
		assert_eq!(count, 1);
	}

	#[rstest]
	fn test_build_deployment_init_container_shares_env_with_main() {
		// Arrange
		let app = make_test_app_with_database();

		// Act
		let deployment = build_deployment(&app, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deployment.spec.unwrap().template.spec.unwrap();
		let main_env = pod_spec.containers[0].env.clone();
		let init_containers = pod_spec.init_containers.unwrap();
		let init_env = init_containers[0].env.clone();

		// Assert
		assert_eq!(main_env, init_env);
	}

	#[rstest]
	fn test_build_deployment_no_runtime_class_without_isolation() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deploy = build_deployment(&app, &Platform::Aws).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert
		assert!(pod_spec.runtime_class_name.is_none());
	}

	#[rstest]
	fn test_build_deployment_sets_runtime_class_for_microvm() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.isolation = Some(IsolationSpec {
			level: IsolationLevel::MicroVM,
			..Default::default()
		});

		// Act
		let deploy = build_deployment(&app, &Platform::Aws).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert
		assert_eq!(pod_spec.runtime_class_name.as_deref(), Some("kata-clh"));
	}

	#[rstest]
	fn test_build_deployment_sets_runtime_class_for_sandbox() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.isolation = Some(IsolationSpec {
			level: IsolationLevel::Sandbox,
			..Default::default()
		});

		// Act
		let deploy = build_deployment(&app, &Platform::Gcp).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert
		assert_eq!(pod_spec.runtime_class_name.as_deref(), Some("gvisor"));
	}

	#[rstest]
	fn test_build_deployment_has_security_context_when_isolated() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.isolation = Some(IsolationSpec {
			level: IsolationLevel::Sandbox,
			..Default::default()
		});

		// Act
		let deploy = build_deployment(&app, &Platform::Aws).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert
		let psc = pod_spec.security_context.unwrap();
		assert_eq!(psc.run_as_non_root, Some(true));
		let container_sc = pod_spec.containers[0].security_context.as_ref().unwrap();
		assert_eq!(container_sc.allow_privilege_escalation, Some(false));
	}
}
