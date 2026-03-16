//! Helper functions to build Kubernetes resources owned by a `ReinhardtApp`.

use std::collections::BTreeMap;

use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{
	Container, ContainerPort, PodSpec, PodTemplateSpec, Service, ServicePort, ServiceSpec,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::{Resource, ResourceExt};
use nuages_types::crd::ReinhardtApp;

/// Standard labels applied to all resources owned by the operator.
pub(crate) fn standard_labels(app: &ReinhardtApp) -> BTreeMap<String, String> {
	BTreeMap::from([
		("app.kubernetes.io/name".to_string(), app.name_any()),
		(
			"app.kubernetes.io/managed-by".to_string(),
			"nuages-operator".to_string(),
		),
		("app.kubernetes.io/instance".to_string(), app.name_any()),
		(
			"paas.nuages.dev/owner".to_string(),
			format!("{}/{}", app.namespace().unwrap_or_default(), app.name_any()),
		),
	])
}

/// Builds a `Deployment` for the given `ReinhardtApp`.
pub(crate) fn build_deployment(app: &ReinhardtApp, namespace: &str) -> Deployment {
	let labels = standard_labels(app);
	let replicas = app.spec.replicas.unwrap_or(1);
	let port = app
		.spec
		.services
		.as_ref()
		.and_then(|s| s.target_port)
		.unwrap_or(8000);

	Deployment {
		metadata: ObjectMeta {
			name: Some(app.name_any()),
			namespace: Some(namespace.to_string()),
			labels: Some(labels.clone()),
			owner_references: Some(vec![app.controller_owner_ref(&()).unwrap()]),
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
					containers: vec![Container {
						name: app.name_any(),
						image: Some(app.spec.image.clone()),
						ports: Some(vec![ContainerPort {
							container_port: port,
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
	}
}

/// Builds a `Service` for the given `ReinhardtApp`.
pub(crate) fn build_service(app: &ReinhardtApp, namespace: &str) -> Service {
	let labels = standard_labels(app);
	let port = app
		.spec
		.services
		.as_ref()
		.and_then(|s| s.port)
		.unwrap_or(80);
	let target_port = app
		.spec
		.services
		.as_ref()
		.and_then(|s| s.target_port)
		.unwrap_or(8000);

	Service {
		metadata: ObjectMeta {
			name: Some(app.name_any()),
			namespace: Some(namespace.to_string()),
			labels: Some(labels.clone()),
			owner_references: Some(vec![app.controller_owner_ref(&()).unwrap()]),
			..Default::default()
		},
		spec: Some(ServiceSpec {
			selector: Some(BTreeMap::from([(
				"app.kubernetes.io/name".to_string(),
				app.name_any(),
			)])),
			ports: Some(vec![ServicePort {
				port,
				target_port: Some(IntOrString::Int(target_port)),
				..Default::default()
			}]),
			..Default::default()
		}),
		..Default::default()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use kube::api::ObjectMeta;
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
				database: None,
				scale: None,
				health: None,
				services: None,
			},
			status: None,
		}
	}

	#[rstest]
	fn test_standard_labels_includes_required_keys() {
		// Arrange
		let app = make_test_app("my-app", "img:v1", None);

		// Act
		let labels = standard_labels(&app);

		// Assert
		assert_eq!(labels.get("app.kubernetes.io/name").unwrap(), "my-app");
		assert_eq!(
			labels.get("app.kubernetes.io/managed-by").unwrap(),
			"nuages-operator"
		);
		assert_eq!(labels.get("app.kubernetes.io/instance").unwrap(), "my-app");
		assert_eq!(
			labels.get("paas.nuages.dev/owner").unwrap(),
			"default/my-app"
		);
	}

	#[rstest]
	fn test_build_deployment_sets_image_and_replicas() {
		// Arrange
		let app = make_test_app("web", "web:latest", Some(3));

		// Act
		let deploy = build_deployment(&app, "default");

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
		let deploy = build_deployment(&app, "default");

		// Assert
		assert_eq!(deploy.spec.unwrap().replicas, Some(1));
	}

	#[rstest]
	fn test_build_deployment_defaults_port_to_8000() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let deploy = build_deployment(&app, "default");

		// Assert
		let container = &deploy.spec.unwrap().template.spec.unwrap().containers[0];
		let port = &container.ports.as_ref().unwrap()[0];
		assert_eq!(port.container_port, 8000);
	}

	#[rstest]
	fn test_build_service_uses_custom_ports() {
		// Arrange
		let mut app = make_test_app("api", "api:v2", None);
		app.spec.services = Some(ServicesSpec {
			port: Some(443),
			target_port: Some(9090),
		});

		// Act
		let svc = build_service(&app, "staging");

		// Assert
		let svc_spec = svc.spec.unwrap();
		let svc_port = &svc_spec.ports.as_ref().unwrap()[0];
		assert_eq!(svc_port.port, 443);
		assert_eq!(svc_port.target_port, Some(IntOrString::Int(9090)));
	}

	#[rstest]
	fn test_build_service_defaults_ports() {
		// Arrange
		let app = make_test_app("web", "web:v1", None);

		// Act
		let svc = build_service(&app, "default");

		// Assert
		let svc_spec = svc.spec.unwrap();
		let svc_port = &svc_spec.ports.as_ref().unwrap()[0];
		assert_eq!(svc_port.port, 80);
		assert_eq!(svc_port.target_port, Some(IntOrString::Int(8000)));
	}
}
