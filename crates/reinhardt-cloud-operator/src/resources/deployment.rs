//! Deployment builder for operator-managed `ReinhardtApp` resources.

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
	Container, ContainerPort, EmptyDirVolumeSource, EnvVar, HTTPGetAction, PodSpec,
	PodTemplateSpec, Probe, ResourceRequirements, Volume, VolumeMount,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::ResourceExt;
use reinhardt_cloud_types::crd::ReinhardtApp;

use super::labels::{Component, owner_reference, standard_labels};
use super::plugins::build_plugin_volumes;
use super::security::context::{build_container_security_context, build_pod_security_context};
use super::security::runtime_class::resolve_runtime_class_name;
use super::validate_port;
use crate::error::Error;
use crate::inference::env_vars::{
	build_core_secret_key_env_vars, build_database_env_vars_from_secret, build_jwt_secret_env_var,
	build_otel_env_vars, build_redis_cache_env_var, build_system_env_vars, merge_env_vars,
};
use crate::inference::pages::ResolvedPagesConfig;
use crate::inference::platform::Platform;

fn build_main_container_probe(
	app: &ReinhardtApp,
	default_port: i32,
) -> Result<Option<Probe>, Error> {
	let Some(health) = app.spec.health.as_ref() else {
		return Ok(None);
	};
	let path = health.path.clone().unwrap_or_else(|| "/health".to_string());
	let port = validate_port("health.port", health.port.unwrap_or(default_port))?;
	let period_seconds = health.interval_seconds.unwrap_or(10);
	if period_seconds < 1 {
		return Err(Error::InvalidProbePeriod {
			field: "health.interval_seconds",
			seconds: period_seconds,
		});
	}

	Ok(Some(Probe {
		http_get: Some(HTTPGetAction {
			path: Some(path),
			port: IntOrString::Int(port),
			..Default::default()
		}),
		period_seconds: Some(period_seconds),
		..Default::default()
	}))
}

/// Builds a `Deployment` for the given `ReinhardtApp`.
///
/// Uses the app's own namespace as the single source of truth.
/// When `pages_config` is provided, adds a collectstatic initContainer,
/// a static-server sidecar container, and a shared emptyDir volume.
/// Returns an error if the owner reference cannot be computed.
pub(crate) fn build_deployment(
	app: &ReinhardtApp,
	pages_config: Option<&ResolvedPagesConfig>,
	platform: &Platform,
) -> Result<Deployment, Error> {
	let labels = standard_labels(app, Component::Web);
	let namespace = super::require_namespace(app)?;
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

	// Build merged environment variables (system + database + user overrides + OTel).
	// Inject database connection env vars when a database will be provisioned,
	// either via an explicit spec.database field or via introspect-derived
	// infrastructure signals (requires_postgresql).
	//
	// The DB host differs between the two provisioning paths:
	// - Explicit spec.database: "{app_name}-db" (headless Service from infer_database_resources)
	// - Introspect path: "{app_name}-postgresql" (Service from reconcile_db_service_resource)
	//
	// Note: user-provided spec.env values take priority over auto-generated DB env vars
	// (including REINHARDT_DATABASE_PASSWORD). This is intentional — users may need to
	// override connection parameters — but plaintext credentials in spec.env are discouraged.
	let app_name = app.name_any();
	let explicit_db = app.spec.database.is_some();
	let introspect_db = app.spec.introspect.as_ref().is_some_and(|i| {
		reinhardt_cloud_core::inference::requires_postgresql(&i.features.infrastructure_signals)
	});
	let needs_db_env = explicit_db || introspect_db;
	let explicit_cache = app.spec.cache.is_some();
	let introspect_cache = app.spec.introspect.as_ref().is_some_and(|i| {
		reinhardt_cloud_core::inference::requires_cache(&i.features.infrastructure_signals)
	});
	let redis_session_backend = app.spec.introspect.as_ref().is_some_and(|i| {
		i.features.infrastructure_signals.session_backend.as_deref() == Some("redis")
	});
	let needs_redis_env = explicit_cache || introspect_cache || redis_session_backend;
	let mut auto_vars = build_system_env_vars();
	// Inject the per-app `core.secret_key` env var unconditionally so the
	// generated `production.toml` can resolve its secret-key interpolation at
	// startup; the value lives in the operator-managed `<app>-core-secret-key`
	// Secret, never in the Pod spec.
	auto_vars.extend(build_core_secret_key_env_vars(&app_name));
	if app.spec.auth.as_ref().is_some_and(|auth| auth.jwt) {
		auto_vars.push(build_jwt_secret_env_var(&app_name));
	}
	if needs_redis_env {
		auto_vars.push(build_redis_cache_env_var(&app_name));
	}
	if needs_db_env {
		let db_host = if explicit_db {
			format!("{app_name}-db")
		} else {
			format!("{app_name}-postgresql")
		};
		auto_vars.extend(build_database_env_vars_from_secret(
			app,
			platform,
			&db_host,
			&app.spec.env,
		));
	}
	let mut merged_env = merge_env_vars(&auto_vars, &app.spec.env);
	// Append OTel variables after user-supplied vars. OTel vars are skipped
	// when a user-supplied var with the same name already exists — user-supplied
	// env vars take precedence over operator-injected OTel defaults.
	let otel_vars = build_otel_env_vars(&app_name);
	for v in otel_vars {
		if !merged_env.iter().any(|e| e.name == v.name) {
			merged_env.push(v);
		}
	}

	// dentdelion WASM plugin volumes and mounts. Empty when spec.plugins is
	// absent or empty; callers of build_plugin_configmap provision the
	// backing ConfigMap separately in the reconciler.
	//
	// As of #589, the operator no longer injects an `<app>-settings`
	// ConfigMap volume — each reinhardt-web image ships its own bundled
	// `production.toml` (made self-contained via ${VAR} interpolation in
	// #588), so the application reads settings from its compile-time
	// `CARGO_MANIFEST_DIR/settings` path.
	let (plugin_volumes, plugin_mounts) = build_plugin_volumes(app);
	let mut volumes: Vec<Volume> = plugin_volumes;
	let volume_mounts: Vec<VolumeMount> = plugin_mounts;

	// Init container for database migrations when database will be provisioned
	let mut init_containers: Vec<Container> = if needs_db_env {
		vec![Container {
			name: "migrate".to_string(),
			image: Some(app.spec.image.clone()),
			command: Some(vec!["manage".to_string(), "migrate".to_string()]),
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
		}]
	} else {
		Vec::new()
	};

	let isolated = app.spec.isolation.is_some();

	// Additional containers (sidecars)
	let mut extra_containers: Vec<Container> = Vec::new();

	// Pages: collectstatic initContainer, static-server sidecar, emptyDir volume
	if let Some(config) = pages_config {
		// Add shared emptyDir volume for static files
		volumes.push(Volume {
			name: "static-files".to_string(),
			empty_dir: Some(EmptyDirVolumeSource::default()),
			..Default::default()
		});

		// collectstatic initContainer (after migrate)
		let mut collectstatic_mounts = volume_mounts.clone();
		collectstatic_mounts.push(VolumeMount {
			name: "static-files".to_string(),
			mount_path: config.static_root.clone(),
			..Default::default()
		});

		let mut collectstatic_env = merged_env.clone();
		collectstatic_env.push(EnvVar {
			name: "REINHARDT_STATIC_ROOT".to_string(),
			value: Some(config.static_root.clone()),
			..Default::default()
		});

		init_containers.push(Container {
			name: "collectstatic".to_string(),
			image: Some(app.spec.image.clone()),
			command: Some(vec![
				"manage".to_string(),
				"collectstatic".to_string(),
				"--no-input".to_string(),
			]),
			env: Some(collectstatic_env),
			volume_mounts: Some(collectstatic_mounts),
			..Default::default()
		});

		// Convert server_resources to k8s ResourceRequirements
		let server_resources = ResourceRequirements {
			requests: Some(
				config
					.server_resources
					.requests
					.iter()
					.map(|(k, v)| (k.clone(), Quantity(v.clone())))
					.collect(),
			),
			limits: Some(
				config
					.server_resources
					.limits
					.iter()
					.map(|(k, v)| (k.clone(), Quantity(v.clone())))
					.collect(),
			),
			..Default::default()
		};

		// static-server sidecar container
		extra_containers.push(Container {
			name: "static-server".to_string(),
			image: Some(config.server_image.clone()),
			ports: Some(vec![ContainerPort {
				container_port: 8080,
				name: Some("http-static".to_string()),
				..Default::default()
			}]),
			env: Some(vec![
				EnvVar {
					name: "SERVER_ROOT".to_string(),
					value: Some(config.static_root.clone()),
					..Default::default()
				},
				EnvVar {
					name: "SERVER_PORT".to_string(),
					value: Some("8080".to_string()),
					..Default::default()
				},
				EnvVar {
					name: "SERVER_LOG_LEVEL".to_string(),
					value: Some("info".to_string()),
					..Default::default()
				},
				EnvVar {
					name: "SERVER_COMPRESSION".to_string(),
					value: Some((config.brotli || config.gzip).to_string()),
					..Default::default()
				},
				EnvVar {
					name: "SERVER_COMPRESSION_STATIC".to_string(),
					value: Some((config.brotli || config.gzip).to_string()),
					..Default::default()
				},
				EnvVar {
					name: "SERVER_HEALTH".to_string(),
					value: Some("true".to_string()),
					..Default::default()
				},
			]),
			volume_mounts: Some(vec![VolumeMount {
				name: "static-files".to_string(),
				mount_path: config.static_root.clone(),
				read_only: Some(true),
				..Default::default()
			}]),
			readiness_probe: Some(Probe {
				http_get: Some(HTTPGetAction {
					path: Some("/health".to_string()),
					port: IntOrString::Int(8080),
					..Default::default()
				}),
				initial_delay_seconds: Some(2),
				period_seconds: Some(5),
				..Default::default()
			}),
			resources: Some(server_resources),
			..Default::default()
		});
	}

	let init_containers_opt = if init_containers.is_empty() {
		None
	} else {
		Some(init_containers)
	};
	let main_container_probe = build_main_container_probe(app, port)?;

	let mut containers = vec![Container {
		name: app.name_any(),
		image: Some(app.spec.image.clone()),
		ports: Some(vec![ContainerPort {
			container_port: port,
			..Default::default()
		}]),
		env: Some(merged_env),
		volume_mounts: Some(volume_mounts),
		readiness_probe: main_container_probe.clone(),
		liveness_probe: main_container_probe,
		security_context: if isolated {
			Some(build_container_security_context())
		} else {
			None
		},
		..Default::default()
	}];
	containers.extend(extra_containers);

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
					security_context: if isolated {
						Some(build_pod_security_context())
					} else {
						None
					},
					init_containers: init_containers_opt,
					containers,
					volumes: Some(volumes),
					// Forward spec.imagePullSecrets verbatim so the kubelet can
					// authenticate to private registries when pulling the main
					// application container, the migrate init-container, the
					// collectstatic init-container, and the static-server
					// sidecar — they all share this PodSpec.
					image_pull_secrets: app.spec.image_pull_secrets.clone(),
					// Bind the workload to a per-app KSA when configured.
					// The name resolution is centralized in
					// `service_account::resolved_sa_name` so the SA builder
					// and the PodSpec wiring can never disagree.
					service_account_name: super::service_account::resolved_sa_name(app),
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
	use reinhardt_cloud_types::crd::auth::AuthSpec;
	use reinhardt_cloud_types::crd::cache::{CacheBackend, CacheSpec};
	use reinhardt_cloud_types::crd::database::{DatabaseEngine, DatabaseSpec};
	use reinhardt_cloud_types::crd::isolation::{IsolationLevel, IsolationSpec};
	use reinhardt_cloud_types::crd::{HealthSpec, ReinhardtAppSpec, ServicesSpec};
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
		let deploy =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");

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
		let deploy =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");

		// Assert
		assert_eq!(deploy.spec.unwrap().replicas, Some(1));
	}

	#[rstest]
	fn test_build_deployment_defaults_port_to_8000() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deploy =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");

		// Assert
		let container = &deploy.spec.unwrap().template.spec.unwrap().containers[0];
		let port = &container.ports.as_ref().unwrap()[0];
		assert_eq!(port.container_port, 8000);
	}

	#[rstest]
	fn test_build_deployment_omits_main_container_probe_without_health() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deploy =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");

		// Assert
		let container = &deploy.spec.unwrap().template.spec.unwrap().containers[0];
		assert!(container.readiness_probe.is_none());
		assert!(container.liveness_probe.is_none());
	}

	#[rstest]
	fn test_build_deployment_applies_health_to_main_container_probes() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.health = Some(HealthSpec {
			path: Some("/api/healthz/".to_string()),
			port: Some(8000),
			interval_seconds: Some(10),
		});

		// Act
		let deploy =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");

		// Assert
		let container = &deploy.spec.unwrap().template.spec.unwrap().containers[0];
		for probe in [
			container.readiness_probe.as_ref().unwrap(),
			container.liveness_probe.as_ref().unwrap(),
		] {
			let http = probe.http_get.as_ref().unwrap();
			assert_eq!(http.path.as_deref(), Some("/api/healthz/"));
			assert_eq!(http.port, IntOrString::Int(8000));
			assert_eq!(probe.period_seconds, Some(10));
		}
	}

	#[rstest]
	#[case(0)]
	#[case(-1)]
	fn test_build_deployment_rejects_invalid_health_interval(#[case] interval_seconds: i32) {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.health = Some(HealthSpec {
			path: Some("/api/healthz/".to_string()),
			port: Some(8000),
			interval_seconds: Some(interval_seconds),
		});

		// Act
		let result = build_deployment(&app, None, &Platform::Onpremise);

		// Assert
		assert!(
			matches!(
				result,
				Err(Error::InvalidProbePeriod {
					field: "health.interval_seconds",
					seconds
				}) if seconds == interval_seconds
			),
			"expected invalid probe period error, got {result:?}"
		);
	}

	#[rstest]
	fn test_build_deployment_uses_app_namespace() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.metadata.namespace = Some("staging".to_string());

		// Act
		let deploy =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");

		// Assert
		assert_eq!(deploy.metadata.namespace.as_deref(), Some("staging"));
	}

	#[rstest]
	fn test_build_deployment_returns_error_without_uid() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.metadata.uid = None;

		// Act
		let result = build_deployment(&app, None, &Platform::Onpremise);

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
		let result = build_deployment(&app, None, &Platform::Onpremise);

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
		let result = build_deployment(&app, None, &Platform::Onpremise);

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
		let result = build_deployment(&app, None, &Platform::Onpremise);

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
		let deployment =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deployment.spec.unwrap().template.spec.unwrap();

		// Assert
		let init_containers = pod_spec.init_containers.unwrap();
		assert_eq!(init_containers.len(), 1);
		assert_eq!(init_containers[0].name, "migrate");
		let expected_command = vec!["manage".to_string(), "migrate".to_string()];
		assert_eq!(init_containers[0].command.as_ref(), Some(&expected_command));
	}

	#[rstest]
	fn test_build_deployment_no_init_container_without_database() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deployment =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deployment.spec.unwrap().template.spec.unwrap();

		// Assert
		assert!(pod_spec.init_containers.is_none() || pod_spec.init_containers.unwrap().is_empty());
	}

	#[rstest]
	fn test_build_deployment_has_no_settings_volume_or_mount() {
		// Arrange — after #589, the operator stops emitting an
		// `<app>-settings` ConfigMap and the corresponding volume/mount;
		// the application reads its bundled production.toml directly.
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deployment =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deployment.spec.unwrap().template.spec.unwrap();

		// Assert — no Volume named "settings"
		let no_settings_volume = pod_spec
			.volumes
			.as_ref()
			.is_none_or(|vs| !vs.iter().any(|v| v.name == "settings"));
		assert!(
			no_settings_volume,
			"Pod spec must not include a `settings` Volume after #589",
		);

		// Assert — no VolumeMount at /etc/reinhardt-cloud/settings on any
		// container or init container.
		for container in &pod_spec.containers {
			if let Some(mounts) = &container.volume_mounts {
				assert!(
					!mounts
						.iter()
						.any(|m| m.mount_path == "/etc/reinhardt-cloud/settings"),
					"container {:?} must not mount /etc/reinhardt-cloud/settings",
					container.name,
				);
			}
		}
		for init in pod_spec.init_containers.as_deref().unwrap_or(&[]) {
			if let Some(mounts) = &init.volume_mounts {
				assert!(
					!mounts
						.iter()
						.any(|m| m.mount_path == "/etc/reinhardt-cloud/settings"),
					"init container {:?} must not mount /etc/reinhardt-cloud/settings",
					init.name,
				);
			}
		}
	}

	#[rstest]
	fn test_build_deployment_injects_system_env_vars() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deployment =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let containers = deployment.spec.unwrap().template.spec.unwrap().containers;
		let env = containers[0].env.as_ref().unwrap();

		// Assert — `REINHARDT_ENV` is the only system env var the operator
		// auto-injects after #589. `REINHARDT_CLOUD_CONFIG_DIR` was removed
		// alongside the settings ConfigMap.
		assert!(
			env.iter()
				.any(|e| e.name == "REINHARDT_ENV" && e.value.as_deref() == Some("production"))
		);
		assert!(
			!env.iter().any(|e| e.name == "REINHARDT_CLOUD_CONFIG_DIR"),
			"REINHARDT_CLOUD_CONFIG_DIR must not be auto-injected after #589",
		);
	}

	#[rstest]
	fn test_build_deployment_injects_core_secret_key_aliases() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deployment =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let containers = deployment.spec.unwrap().template.spec.unwrap().containers;
		let env = containers[0].env.as_ref().unwrap();

		// Assert
		for name in ["REINHARDT_CORE__SECRET_KEY", "REINHARDT_CLOUD_SECRET_KEY"] {
			let var = env.iter().find(|e| e.name == name).expect("env must exist");
			let key_ref = var
				.value_from
				.as_ref()
				.and_then(|vf| vf.secret_key_ref.as_ref())
				.expect("core secret key must be Secret-backed");
			assert_eq!(key_ref.name, "web-core-secret-key");
			assert_eq!(key_ref.key, "secret-key");
		}
	}

	#[rstest]
	fn test_build_deployment_injects_jwt_secret_when_auth_jwt_enabled() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.auth = Some(AuthSpec {
			jwt: true,
			oauth: None,
		});

		// Act
		let deployment =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let containers = deployment.spec.unwrap().template.spec.unwrap().containers;
		let env = containers[0].env.as_ref().unwrap();

		// Assert
		let var = env
			.iter()
			.find(|e| e.name == "REINHARDT_CLOUD_JWT_SECRET")
			.expect("JWT secret env must exist");
		let key_ref = var
			.value_from
			.as_ref()
			.and_then(|vf| vf.secret_key_ref.as_ref())
			.expect("JWT secret must be Secret-backed");
		assert_eq!(key_ref.name, "web-jwt-secret");
		assert_eq!(key_ref.key, "jwt-secret");
	}

	#[rstest]
	fn test_build_deployment_injects_redis_url_when_cache_enabled() {
		// Arrange
		let mut app = make_test_app_with_database();
		app.spec.cache = Some(CacheSpec {
			backend: CacheBackend::Redis,
			instance_type: None,
		});

		// Act
		let deployment =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deployment.spec.unwrap().template.spec.unwrap();
		let main_env = pod_spec.containers[0].env.as_ref().unwrap();
		let init_env = pod_spec.init_containers.as_ref().unwrap()[0]
			.env
			.as_ref()
			.unwrap();

		// Assert
		for env in [main_env, init_env] {
			let var = env
				.iter()
				.find(|e| e.name == "REINHARDT_CLOUD_REDIS_URL")
				.expect("Redis URL env must exist");
			assert_eq!(var.value.as_deref(), Some("redis://web-redis:6379/0"));
			assert!(var.value_from.is_none());
		}
	}

	#[rstest]
	fn test_build_deployment_user_env_overrides_system() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.env = BTreeMap::from([("REINHARDT_ENV".to_string(), "staging".to_string())]);

		// Act
		let deployment =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
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
		let deployment =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
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
		let deployment =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
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
	fn test_build_deployment_user_env_var_overrides_reinhardt_env() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec
			.env
			.insert("REINHARDT_ENV".to_string(), "development".to_string());

		// Act
		let deployment =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
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
		let deployment =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deployment.spec.unwrap().template.spec.unwrap();
		let main_env = pod_spec.containers[0].env.clone();
		let init_containers = pod_spec.init_containers.unwrap();
		let init_env = init_containers[0].env.clone();

		// Assert
		assert_eq!(main_env, init_env);
	}

	// ── Pages sidecar tests ───────────────────────────────────────────

	fn make_default_pages_config() -> crate::inference::pages::ResolvedPagesConfig {
		crate::inference::pages::ResolvedPagesConfig::default()
	}

	#[rstest]
	fn test_build_deployment_without_pages_unchanged() {
		// Arrange
		let app = make_test_app("app", "img:v1", None);

		// Act
		let dep = build_deployment(&app, None, &Platform::Aws).unwrap();
		let spec = dep.spec.unwrap();
		let pod_spec = spec.template.spec.unwrap();

		// Assert
		assert_eq!(pod_spec.containers.len(), 1);
	}

	#[rstest]
	fn test_build_deployment_with_pages_adds_sidecar() {
		// Arrange
		let app = make_test_app("app", "img:v1", None);
		let pages = make_default_pages_config();

		// Act
		let dep = build_deployment(&app, Some(&pages), &Platform::Onpremise).unwrap();
		let pod_spec = dep.spec.unwrap().template.spec.unwrap();

		// Assert
		assert_eq!(pod_spec.containers.len(), 2);
		let sidecar = &pod_spec.containers[1];
		assert_eq!(sidecar.name, "static-server");
		assert_eq!(
			sidecar.image.as_deref(),
			Some("joseluisq/static-web-server:2-alpine")
		);
		assert!(
			sidecar
				.env
				.as_ref()
				.unwrap()
				.iter()
				.any(|env| { env.name == "SERVER_HEALTH" && env.value.as_deref() == Some("true") })
		);
	}

	#[rstest]
	fn test_build_deployment_with_pages_adds_collectstatic_init_container() {
		// Arrange
		let app = make_test_app("app", "img:v1", None);
		let pages = make_default_pages_config();

		// Act
		let dep = build_deployment(&app, Some(&pages), &Platform::Onpremise).unwrap();
		let pod_spec = dep.spec.unwrap().template.spec.unwrap();
		let inits = pod_spec.init_containers.unwrap();

		// Assert
		let collectstatic = inits
			.iter()
			.find(|c| c.name == "collectstatic")
			.expect("collectstatic init container should be present");
		let expected_command = vec![
			"manage".to_string(),
			"collectstatic".to_string(),
			"--no-input".to_string(),
		];
		assert_eq!(collectstatic.command.as_ref(), Some(&expected_command));
	}

	#[rstest]
	fn test_build_deployment_with_pages_adds_emptydir_volume() {
		// Arrange
		let app = make_test_app("app", "img:v1", None);
		let pages = make_default_pages_config();

		// Act
		let dep = build_deployment(&app, Some(&pages), &Platform::Onpremise).unwrap();
		let pod_spec = dep.spec.unwrap().template.spec.unwrap();
		let volumes = pod_spec.volumes.unwrap();

		// Assert
		assert!(volumes.iter().any(|v| v.name == "static-files"));
	}

	#[rstest]
	fn test_build_deployment_pages_custom_static_root() {
		// Arrange
		let app = make_test_app("app", "img:v1", None);
		let mut pages = make_default_pages_config();
		pages.static_root = "/opt/static".to_string();

		// Act
		let dep = build_deployment(&app, Some(&pages), &Platform::Onpremise).unwrap();
		let pod_spec = dep.spec.unwrap().template.spec.unwrap();
		let sidecar = &pod_spec.containers[1];
		let server_root = sidecar
			.env
			.as_ref()
			.unwrap()
			.iter()
			.find(|e| e.name == "SERVER_ROOT")
			.unwrap();

		// Assert
		assert_eq!(server_root.value.as_deref(), Some("/opt/static"));
	}

	#[rstest]
	fn test_build_deployment_pages_custom_server_image() {
		// Arrange
		let app = make_test_app("app", "img:v1", None);
		let mut pages = make_default_pages_config();
		pages.server_image = "custom:v2".to_string();

		// Act
		let dep = build_deployment(&app, Some(&pages), &Platform::Onpremise).unwrap();
		let pod_spec = dep.spec.unwrap().template.spec.unwrap();
		let sidecar = &pod_spec.containers[1];

		// Assert
		assert_eq!(sidecar.image.as_deref(), Some("custom:v2"));
	}

	#[rstest]
	fn test_build_deployment_pages_sidecar_readiness_probe() {
		// Arrange
		let app = make_test_app("app", "img:v1", None);
		let pages = make_default_pages_config();

		// Act
		let dep = build_deployment(&app, Some(&pages), &Platform::Onpremise).unwrap();
		let pod_spec = dep.spec.unwrap().template.spec.unwrap();
		let sidecar = &pod_spec.containers[1];

		// Assert
		let probe = sidecar.readiness_probe.as_ref().unwrap();
		let http = probe.http_get.as_ref().unwrap();
		assert_eq!(http.path.as_deref(), Some("/health"));
	}

	// ── Isolation tests ───────────────────────────────────────────

	#[rstest]
	fn test_build_deployment_no_runtime_class_without_isolation() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deploy = build_deployment(&app, None, &Platform::Aws).expect("build should succeed");
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
		let deploy = build_deployment(&app, None, &Platform::Aws).expect("build should succeed");
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
		let deploy = build_deployment(&app, None, &Platform::Gcp).expect("build should succeed");
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
		let deploy = build_deployment(&app, None, &Platform::Aws).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert
		let psc = pod_spec.security_context.unwrap();
		assert_eq!(psc.run_as_non_root, Some(true));
		let container_sc = pod_spec.containers[0].security_context.as_ref().unwrap();
		assert_eq!(container_sc.allow_privilege_escalation, Some(false));
	}

	// --- service_account_name wiring ---

	#[rstest]
	fn test_pod_spec_service_account_name_unset_when_spec_none() {
		// Arrange — no spec.service_account configured
		let app = make_test_app("web", "web:latest", None);

		// Act
		let deploy =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert — existing behavior preserved
		assert_eq!(pod_spec.service_account_name, None);
	}

	#[rstest]
	fn test_pod_spec_service_account_name_default_when_create_true() {
		// Arrange — create=true, no explicit name
		use reinhardt_cloud_types::crd::service_account::ServiceAccountSpec;
		let mut app = make_test_app("web", "web:latest", None);
		app.spec.service_account = Some(ServiceAccountSpec {
			create: true,
			name: None,
			annotations: BTreeMap::new(),
		});

		// Act
		let deploy =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert — `{app}-app` default name is wired in
		assert_eq!(pod_spec.service_account_name.as_deref(), Some("web-app"));
	}

	#[rstest]
	fn test_pod_spec_service_account_name_explicit_when_create_true() {
		// Arrange — create=true with explicit name
		use reinhardt_cloud_types::crd::service_account::ServiceAccountSpec;
		let mut app = make_test_app("web", "web:latest", None);
		app.spec.service_account = Some(ServiceAccountSpec {
			create: true,
			name: Some("my-sa".to_string()),
			annotations: BTreeMap::new(),
		});

		// Act
		let deploy =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert
		assert_eq!(pod_spec.service_account_name.as_deref(), Some("my-sa"));
	}

	#[rstest]
	fn test_pod_spec_service_account_name_explicit_when_create_false() {
		// Arrange — user pre-created the KSA themselves and supplied the name
		use reinhardt_cloud_types::crd::service_account::ServiceAccountSpec;
		let mut app = make_test_app("web", "web:latest", None);
		app.spec.service_account = Some(ServiceAccountSpec {
			create: false,
			name: Some("user-managed".to_string()),
			annotations: BTreeMap::new(),
		});

		// Act
		let deploy =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert — the supplied name is used; the operator does not create the SA
		assert_eq!(
			pod_spec.service_account_name.as_deref(),
			Some("user-managed")
		);
	}

	#[rstest]
	fn test_pod_spec_service_account_name_unset_when_create_false_and_no_name() {
		// Arrange — ambiguous: don't create, no name → fall back to namespace default SA
		use reinhardt_cloud_types::crd::service_account::ServiceAccountSpec;
		let mut app = make_test_app("web", "web:latest", None);
		app.spec.service_account = Some(ServiceAccountSpec {
			create: false,
			name: None,
			annotations: BTreeMap::new(),
		});

		// Act
		let deploy =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert
		assert_eq!(pod_spec.service_account_name, None);
	}

	// ── Image pull secrets tests ───────────────────────────────────────────

	#[rstest]
	fn test_build_deployment_image_pull_secrets_none_when_unset() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deploy =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert
		assert!(pod_spec.image_pull_secrets.is_none());
	}

	#[rstest]
	fn test_build_deployment_image_pull_secrets_single_passthrough() {
		use k8s_openapi::api::core::v1::LocalObjectReference;

		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.image_pull_secrets = Some(vec![LocalObjectReference {
			name: "regcred".to_string(),
		}]);

		// Act
		let deploy =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert
		let pull_secrets = pod_spec
			.image_pull_secrets
			.expect("image_pull_secrets should be set");
		assert_eq!(pull_secrets.len(), 1);
		assert_eq!(pull_secrets[0].name, "regcred");
	}

	#[rstest]
	fn test_build_deployment_image_pull_secrets_multiple_passthrough() {
		use k8s_openapi::api::core::v1::LocalObjectReference;

		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.image_pull_secrets = Some(vec![
			LocalObjectReference {
				name: "regcred-primary".to_string(),
			},
			LocalObjectReference {
				name: "regcred-fallback".to_string(),
			},
		]);

		// Act
		let deploy =
			build_deployment(&app, None, &Platform::Onpremise).expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert
		let pull_secrets = pod_spec
			.image_pull_secrets
			.expect("image_pull_secrets should be set");
		assert_eq!(pull_secrets.len(), 2);
		assert_eq!(pull_secrets[0].name, "regcred-primary");
		assert_eq!(pull_secrets[1].name, "regcred-fallback");
	}

	#[rstest]
	fn test_build_deployment_image_pull_secrets_apply_to_pages_pod_spec() {
		use k8s_openapi::api::core::v1::LocalObjectReference;

		// Arrange — a pages-enabled deployment shares the same PodSpec
		// across the main container, the static-server sidecar, and the
		// migrate / collectstatic init-containers, so a single pod-level
		// imagePullSecrets covers them all.
		let mut app = make_test_app_with_database();
		app.spec.image_pull_secrets = Some(vec![LocalObjectReference {
			name: "regcred".to_string(),
		}]);
		let pages = make_default_pages_config();

		// Act
		let deploy = build_deployment(&app, Some(&pages), &Platform::Onpremise)
			.expect("build should succeed");
		let pod_spec = deploy.spec.unwrap().template.spec.unwrap();

		// Assert
		let pull_secrets = pod_spec
			.image_pull_secrets
			.expect("image_pull_secrets should be set");
		assert_eq!(pull_secrets.len(), 1);
		assert_eq!(pull_secrets[0].name, "regcred");
		// Sanity check: the PodSpec really does host the additional containers.
		let init_containers = pod_spec.init_containers.as_ref().unwrap();
		assert!(init_containers.iter().any(|c| c.name == "migrate"));
		assert!(init_containers.iter().any(|c| c.name == "collectstatic"));
		assert!(
			pod_spec
				.containers
				.iter()
				.any(|c| c.name == "static-server")
		);
	}
}
