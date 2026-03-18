//! Redis cache resource builders for operator-managed cache instances.

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
	Container, ContainerPort, PodSpec, PodTemplateSpec, ResourceRequirements, Service, ServicePort,
	ServiceSpec,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use kube::ResourceExt;
use nuages_types::crd::ReinhardtApp;

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;

/// Builds a `Deployment` running Redis for the given `ReinhardtApp`.
///
/// Uses `redis:7-alpine` with a single replica and conservative resource limits.
pub(crate) fn build_cache_deployment(app: &ReinhardtApp) -> Result<Deployment, Error> {
	let labels = standard_labels(app, Component::Cache);
	let namespace = app.namespace().unwrap_or_default();
	let owner_ref = owner_reference(app)?;
	let app_name = app.name_any();
	let deploy_name = format!("{}-redis", app_name);

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
						"cache".to_string(),
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
					containers: vec![Container {
						name: "redis".to_string(),
						image: Some("redis:7-alpine".to_string()),
						ports: Some(vec![ContainerPort {
							container_port: 6379,
							name: Some("redis".to_string()),
							..Default::default()
						}]),
						resources: Some(ResourceRequirements {
							requests: Some(BTreeMap::from([
								("memory".to_string(), Quantity("64Mi".to_string())),
								("cpu".to_string(), Quantity("50m".to_string())),
							])),
							limits: Some(BTreeMap::from([(
								"memory".to_string(),
								Quantity("128Mi".to_string()),
							)])),
							..Default::default()
						}),
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

/// Builds a `Service` exposing Redis for the given `ReinhardtApp`.
///
/// Targets port 6379 and selects pods by app name and cache component labels.
pub(crate) fn build_cache_service(app: &ReinhardtApp) -> Result<Service, Error> {
	let labels = standard_labels(app, Component::Cache);
	let namespace = app.namespace().unwrap_or_default();
	let owner_ref = owner_reference(app)?;
	let app_name = app.name_any();

	Ok(Service {
		metadata: ObjectMeta {
			name: Some(format!("{}-redis", app_name)),
			namespace: Some(namespace),
			labels: Some(labels),
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		spec: Some(ServiceSpec {
			type_: Some("ClusterIP".to_string()),
			selector: Some(BTreeMap::from([
				("app.kubernetes.io/name".to_string(), app_name),
				(
					"app.kubernetes.io/component".to_string(),
					"cache".to_string(),
				),
			])),
			ports: Some(vec![ServicePort {
				port: 6379,
				target_port: Some(
					k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(6379),
				),
				name: Some("redis".to_string()),
				..Default::default()
			}]),
			..Default::default()
		}),
		..Default::default()
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	fn test_app(name: &str) -> ReinhardtApp {
		let json = serde_json::json!({
			"apiVersion": "paas.nuages.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": name, "namespace": "default", "uid": "test-uid" },
			"spec": { "image": "myapp:latest" }
		});
		serde_json::from_value(json).unwrap()
	}

	#[rstest]
	fn test_build_cache_deployment_name() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let deploy = build_cache_deployment(&app).expect("build should succeed");

		// Assert
		assert_eq!(deploy.metadata.name.as_deref(), Some("myapp-redis"));
	}

	#[rstest]
	fn test_build_cache_deployment_image() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let deploy = build_cache_deployment(&app).expect("build should succeed");
		let containers = &deploy.spec.unwrap().template.spec.unwrap().containers;

		// Assert
		assert_eq!(containers[0].image.as_deref(), Some("redis:7-alpine"));
	}

	#[rstest]
	fn test_build_cache_deployment_port() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let deploy = build_cache_deployment(&app).expect("build should succeed");
		let containers = &deploy.spec.unwrap().template.spec.unwrap().containers;
		let ports = containers[0].ports.as_ref().unwrap();

		// Assert
		assert_eq!(ports.len(), 1);
		assert_eq!(ports[0].container_port, 6379);
		assert_eq!(ports[0].name.as_deref(), Some("redis"));
	}

	#[rstest]
	fn test_build_cache_deployment_resources() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let deploy = build_cache_deployment(&app).expect("build should succeed");
		let containers = &deploy.spec.unwrap().template.spec.unwrap().containers;
		let resources = containers[0].resources.as_ref().unwrap();

		// Assert
		let requests = resources.requests.as_ref().unwrap();
		assert_eq!(requests.get("memory").unwrap().0, "64Mi");
		assert_eq!(requests.get("cpu").unwrap().0, "50m");

		let limits = resources.limits.as_ref().unwrap();
		assert_eq!(limits.get("memory").unwrap().0, "128Mi");
	}

	#[rstest]
	fn test_build_cache_deployment_component_label() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let deploy = build_cache_deployment(&app).expect("build should succeed");
		let labels = deploy.metadata.labels.as_ref().unwrap();

		// Assert
		assert_eq!(labels.get("app.kubernetes.io/component").unwrap(), "cache");
	}

	#[rstest]
	fn test_build_cache_service_name() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let svc = build_cache_service(&app).expect("build should succeed");

		// Assert
		assert_eq!(svc.metadata.name.as_deref(), Some("myapp-redis"));
	}

	#[rstest]
	fn test_build_cache_service_port() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let svc = build_cache_service(&app).expect("build should succeed");
		let ports = svc.spec.unwrap().ports.unwrap();

		// Assert
		assert_eq!(ports.len(), 1);
		assert_eq!(ports[0].port, 6379);
		assert_eq!(
			ports[0].target_port,
			Some(k8s_openapi::apimachinery::pkg::util::intstr::IntOrString::Int(6379))
		);
	}
}
