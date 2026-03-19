//! Service builder for operator-managed `ReinhardtApp` resources.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{Service, ServicePort, ServiceSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::ResourceExt;
use nuages_types::crd::ReinhardtApp;

use super::labels::{Component, owner_reference, standard_labels};
use super::validate_port;
use crate::error::Error;

/// Builds a `Service` for the given `ReinhardtApp`.
///
/// Uses the app's own namespace as the single source of truth.
/// When `pages_enabled` is true, the service exposes two ports:
/// `http-app` (the application port) and `http-static` (port 8080 for the
/// pages sidecar). Returns an error if the owner reference cannot be computed.
pub(crate) fn build_service(app: &ReinhardtApp, pages_enabled: bool) -> Result<Service, Error> {
	let labels = standard_labels(app, Component::Web);
	let namespace = app.namespace().unwrap_or_default();
	let port = validate_port(
		"port",
		app.spec
			.services
			.as_ref()
			.and_then(|s| s.port)
			.unwrap_or(80),
	)?;
	let target_port = validate_port(
		"target_port",
		app.spec
			.services
			.as_ref()
			.and_then(|s| s.target_port)
			.unwrap_or(8000),
	)?;

	let owner_ref = owner_reference(app)?;

	let mut ports = vec![ServicePort {
		name: if pages_enabled {
			Some("http-app".to_string())
		} else {
			None
		},
		port,
		target_port: Some(IntOrString::Int(target_port)),
		..Default::default()
	}];

	if pages_enabled {
		ports.push(ServicePort {
			name: Some("http-static".to_string()),
			port: 8080,
			target_port: Some(IntOrString::Int(8080)),
			..Default::default()
		});
	}

	Ok(Service {
		metadata: ObjectMeta {
			name: Some(app.name_any()),
			namespace: Some(namespace),
			labels: Some(labels.clone()),
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		spec: Some(ServiceSpec {
			selector: Some(BTreeMap::from([(
				"app.kubernetes.io/name".to_string(),
				app.name_any(),
			)])),
			ports: Some(ports),
			..Default::default()
		}),
		..Default::default()
	})
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
				..Default::default()
			},
			status: None,
		}
	}

	#[rstest]
	fn test_build_service_uses_custom_ports() {
		// Arrange
		let mut app = make_test_app("api", "api:v2", None);
		app.spec.services = Some(ServicesSpec {
			port: Some(443),
			target_port: Some(9090),
			ingress_host: None,
		});

		// Act
		let svc = build_service(&app, false).expect("build should succeed");

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
		let svc = build_service(&app, false).expect("build should succeed");

		// Assert
		let svc_spec = svc.spec.unwrap();
		let svc_port = &svc_spec.ports.as_ref().unwrap()[0];
		assert_eq!(svc_port.port, 80);
		assert_eq!(svc_port.target_port, Some(IntOrString::Int(8000)));
	}

	#[rstest]
	fn test_build_service_uses_app_namespace() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.metadata.namespace = Some("production".to_string());

		// Act
		let svc = build_service(&app, false).expect("build should succeed");

		// Assert
		assert_eq!(svc.metadata.namespace.as_deref(), Some("production"));
	}

	#[rstest]
	fn test_build_service_returns_error_without_uid() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.metadata.uid = None;

		// Act
		let result = build_service(&app, false);

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_build_service_rejects_port_zero() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.services = Some(ServicesSpec {
			port: Some(0),
			target_port: None,
			ingress_host: None,
		});

		// Act
		let result = build_service(&app, false);

		// Assert
		assert!(result.is_err());
		let err = result.unwrap_err().to_string();
		assert_eq!(
			err,
			"invalid port 0 for field 'port': must be between 1 and 65535"
		);
	}

	#[rstest]
	fn test_build_service_rejects_port_above_65535() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.services = Some(ServicesSpec {
			port: Some(65536),
			target_port: None,
			ingress_host: None,
		});

		// Act
		let result = build_service(&app, false);

		// Assert
		assert!(result.is_err());
		let err = result.unwrap_err().to_string();
		assert_eq!(
			err,
			"invalid port 65536 for field 'port': must be between 1 and 65535"
		);
	}

	#[rstest]
	fn test_build_service_rejects_invalid_target_port() {
		// Arrange
		let mut app = make_test_app("web", "web:v1", None);
		app.spec.services = Some(ServicesSpec {
			port: Some(80),
			target_port: Some(70000),
			ingress_host: None,
		});

		// Act
		let result = build_service(&app, false);

		// Assert
		assert!(result.is_err());
		let err = result.unwrap_err().to_string();
		assert_eq!(
			err,
			"invalid port 70000 for field 'target_port': must be between 1 and 65535"
		);
	}

	#[rstest]
	fn test_build_service_with_pages_has_two_ports() {
		// Arrange
		let app = make_test_app("app", "app:v1", None);

		// Act
		let svc = build_service(&app, true).unwrap();
		let ports = svc.spec.unwrap().ports.unwrap();

		// Assert
		assert_eq!(ports.len(), 2);
		assert_eq!(ports[0].name.as_deref(), Some("http-app"));
		assert_eq!(ports[0].port, 80);
		assert_eq!(ports[1].name.as_deref(), Some("http-static"));
		assert_eq!(ports[1].port, 8080);
	}

	#[rstest]
	fn test_build_service_without_pages_single_port() {
		// Arrange
		let app = make_test_app("app", "app:v1", None);

		// Act
		let svc = build_service(&app, false).unwrap();
		let ports = svc.spec.unwrap().ports.unwrap();

		// Assert
		assert_eq!(ports.len(), 1);
	}
}
