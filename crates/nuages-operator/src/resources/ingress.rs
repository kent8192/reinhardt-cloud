//! Ingress builder for operator-managed `ReinhardtApp` resources.

use k8s_openapi::api::networking::v1::{
	HTTPIngressPath, HTTPIngressRuleValue, Ingress, IngressBackend, IngressRule,
	IngressServiceBackend, IngressSpec, ServiceBackendPort,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use nuages_types::crd::ReinhardtApp;
use nuages_types::introspect::RouteMetadata;

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;

/// Builds an `Ingress` for the given `ReinhardtApp` from introspected route metadata.
///
/// Each [`RouteMetadata`] entry becomes an `HTTPIngressPath` with `Prefix` path type,
/// routing to the app's Service on the specified port.
pub(crate) fn build_ingress(
	app: &ReinhardtApp,
	routes: &[RouteMetadata],
	app_port: u16,
	host: Option<&str>,
) -> Result<Ingress, Error> {
	let labels = standard_labels(app, Component::Ingress);
	let namespace = app.namespace().unwrap_or_default();
	let owner_ref = owner_reference(app)?;
	let app_name = app.name_any();

	let paths: Vec<HTTPIngressPath> = routes
		.iter()
		.map(|route| HTTPIngressPath {
			path: Some(route.path.clone()),
			path_type: "Prefix".to_string(),
			backend: IngressBackend {
				service: Some(IngressServiceBackend {
					name: app_name.clone(),
					port: Some(ServiceBackendPort {
						number: Some(i32::from(app_port)),
						..Default::default()
					}),
				}),
				..Default::default()
			},
		})
		.collect();

	let rule = IngressRule {
		host: host.map(String::from),
		http: Some(HTTPIngressRuleValue { paths }),
	};

	Ok(Ingress {
		metadata: ObjectMeta {
			name: Some(app_name),
			namespace: Some(namespace),
			labels: Some(labels),
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		spec: Some(IngressSpec {
			ingress_class_name: Some("nginx".to_string()),
			rules: Some(vec![rule]),
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
				image: "img:v1".to_string(),
				..Default::default()
			},
			status: None,
		}
	}

	fn make_route(path: &str) -> RouteMetadata {
		RouteMetadata {
			path: path.to_string(),
			methods: vec!["GET".to_string()],
			name: None,
			namespace: None,
		}
	}

	#[rstest]
	fn test_build_ingress_name() {
		// Arrange
		let app = make_test_app("my-app");
		let routes = vec![make_route("/api/")];

		// Act
		let ingress = build_ingress(&app, &routes, 8000, None).expect("build should succeed");

		// Assert
		assert_eq!(ingress.metadata.name.as_deref(), Some("my-app"));
	}

	#[rstest]
	fn test_build_ingress_paths_from_routes() {
		// Arrange
		let app = make_test_app("web");
		let routes = vec![make_route("/api/users/"), make_route("/api/posts/")];

		// Act
		let ingress = build_ingress(&app, &routes, 8080, None).expect("build should succeed");

		// Assert
		let spec = ingress.spec.unwrap();
		let rule = &spec.rules.as_ref().unwrap()[0];
		let paths = &rule.http.as_ref().unwrap().paths;
		assert_eq!(paths.len(), 2);
		assert_eq!(paths[0].path.as_deref(), Some("/api/users/"));
		assert_eq!(paths[1].path.as_deref(), Some("/api/posts/"));
		// Verify backend points to the app service
		let backend = paths[0].backend.service.as_ref().unwrap();
		assert_eq!(backend.name, "web");
		assert_eq!(backend.port.as_ref().unwrap().number, Some(8080));
	}

	#[rstest]
	fn test_build_ingress_with_host() {
		// Arrange
		let app = make_test_app("web");
		let routes = vec![make_route("/")];

		// Act
		let ingress =
			build_ingress(&app, &routes, 80, Some("example.com")).expect("build should succeed");

		// Assert
		let spec = ingress.spec.unwrap();
		let rules = spec.rules.as_ref().unwrap();
		assert_eq!(rules[0].host.as_deref(), Some("example.com"));
	}

	#[rstest]
	fn test_build_ingress_without_host() {
		// Arrange
		let app = make_test_app("web");
		let routes = vec![make_route("/")];

		// Act
		let ingress = build_ingress(&app, &routes, 80, None).expect("build should succeed");

		// Assert
		let spec = ingress.spec.unwrap();
		let rules = spec.rules.as_ref().unwrap();
		assert!(rules[0].host.is_none());
	}

	#[rstest]
	fn test_build_ingress_empty_routes() {
		// Arrange
		let app = make_test_app("web");
		let routes: Vec<RouteMetadata> = vec![];

		// Act
		let ingress = build_ingress(&app, &routes, 8000, None).expect("build should succeed");

		// Assert
		let spec = ingress.spec.unwrap();
		let rules = spec.rules.as_ref().unwrap();
		let paths = &rules[0].http.as_ref().unwrap().paths;
		assert!(paths.is_empty());
	}
}
