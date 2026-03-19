//! gRPC Service builder for operator-managed `ReinhardtApp` resources.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{Service, ServicePort, ServiceSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use k8s_openapi::apimachinery::pkg::util::intstr::IntOrString;
use kube::ResourceExt;
use nuages_types::crd::ReinhardtApp;

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;

/// Default gRPC port used when no custom port is configured.
const GRPC_PORT: i32 = 50051;

/// Builds a gRPC `Service` for the given `ReinhardtApp`.
///
/// The service exposes port 50051 with `appProtocol: "kubernetes.io/h2c"` for
/// HTTP/2 cleartext, targeting the web component container. The service name
/// is `{app_name}-grpc`.
pub(crate) fn build_grpc_service(app: &ReinhardtApp) -> Result<Service, Error> {
	let labels = standard_labels(app, Component::Web);
	let namespace = app.namespace().unwrap_or_default();
	let owner_ref = owner_reference(app)?;

	Ok(Service {
		metadata: ObjectMeta {
			name: Some(format!("{}-grpc", app.name_any())),
			namespace: Some(namespace),
			labels: Some(labels),
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		spec: Some(ServiceSpec {
			selector: Some(BTreeMap::from([
				("app.kubernetes.io/name".to_string(), app.name_any()),
				(
					"app.kubernetes.io/component".to_string(),
					Component::Web.as_str().to_string(),
				),
			])),
			ports: Some(vec![ServicePort {
				name: Some("grpc".to_string()),
				port: GRPC_PORT,
				target_port: Some(IntOrString::Int(GRPC_PORT)),
				app_protocol: Some("kubernetes.io/h2c".to_string()),
				..Default::default()
			}]),
			type_: Some("ClusterIP".to_string()),
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

	fn make_test_app(name: &str) -> ReinhardtApp {
		ReinhardtApp {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				..Default::default()
			},
			spec: ReinhardtAppSpec {
				image: "myapp:v1".to_string(),
				..Default::default()
			},
			status: None,
		}
	}

	#[rstest]
	fn test_build_grpc_service_name() {
		// Arrange
		let app = make_test_app("myapp");

		// Act
		let svc = build_grpc_service(&app).expect("build should succeed");

		// Assert
		assert_eq!(svc.metadata.name.as_deref(), Some("myapp-grpc"));
	}

	#[rstest]
	fn test_build_grpc_service_port() {
		// Arrange
		let app = make_test_app("myapp");

		// Act
		let svc = build_grpc_service(&app).expect("build should succeed");

		// Assert
		let svc_spec = svc.spec.unwrap();
		let svc_port = &svc_spec.ports.as_ref().unwrap()[0];
		assert_eq!(svc_port.port, 50051);
		assert_eq!(svc_port.target_port, Some(IntOrString::Int(50051)));
		assert_eq!(svc_port.name.as_deref(), Some("grpc"));
	}

	#[rstest]
	fn test_build_grpc_service_app_protocol() {
		// Arrange
		let app = make_test_app("myapp");

		// Act
		let svc = build_grpc_service(&app).expect("build should succeed");

		// Assert
		let svc_spec = svc.spec.unwrap();
		let svc_port = &svc_spec.ports.as_ref().unwrap()[0];
		assert_eq!(svc_port.app_protocol.as_deref(), Some("kubernetes.io/h2c"));
	}

	#[rstest]
	fn test_build_grpc_service_selector() {
		// Arrange
		let app = make_test_app("myapp");

		// Act
		let svc = build_grpc_service(&app).expect("build should succeed");

		// Assert
		let selector = svc.spec.unwrap().selector.unwrap();
		assert_eq!(selector.get("app.kubernetes.io/name").unwrap(), "myapp");
		assert_eq!(selector.get("app.kubernetes.io/component").unwrap(), "web");
	}

	#[rstest]
	fn test_build_grpc_service_type_is_cluster_ip() {
		// Arrange
		let app = make_test_app("myapp");

		// Act
		let svc = build_grpc_service(&app).expect("build should succeed");

		// Assert
		assert_eq!(svc.spec.unwrap().type_.as_deref(), Some("ClusterIP"));
	}

	#[rstest]
	fn test_build_grpc_service_returns_error_without_uid() {
		// Arrange
		let mut app = make_test_app("myapp");
		app.metadata.uid = None;

		// Act
		let result = build_grpc_service(&app);

		// Assert
		assert!(result.is_err());
	}
}
