//! Ingress builder for operator-managed `ReinhardtApp` resources.

use std::collections::BTreeMap;

use k8s_openapi::api::networking::v1::{
	HTTPIngressPath, HTTPIngressRuleValue, Ingress, IngressBackend, IngressRule,
	IngressServiceBackend, IngressSpec, ServiceBackendPort,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use nuages_types::crd::ReinhardtApp;
use nuages_types::introspect::{InfraSignals, RouteMetadata};

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;
use crate::inference::pages::ResolvedPagesConfig;

/// Builds an `Ingress` for the given `ReinhardtApp` from introspected route metadata.
///
/// Each [`RouteMetadata`] entry becomes an `HTTPIngressPath` with `Prefix` path type,
/// routing to the app's Service on the specified port.
/// Builds nginx-ingress annotations from infrastructure signals.
///
/// WebSocket signals add long proxy timeouts and sticky upstream hashing.
/// gRPC signals switch the backend protocol to `GRPC`.
fn build_annotations(signals: Option<&InfraSignals>) -> Option<BTreeMap<String, String>> {
	let mut annotations = BTreeMap::new();

	if let Some(signals) = signals {
		if signals.websocket {
			annotations.insert(
				"nginx.ingress.kubernetes.io/proxy-read-timeout".to_string(),
				"3600".to_string(),
			);
			annotations.insert(
				"nginx.ingress.kubernetes.io/proxy-send-timeout".to_string(),
				"3600".to_string(),
			);
			annotations.insert(
				"nginx.ingress.kubernetes.io/upstream-hash-by".to_string(),
				"$remote_addr".to_string(),
			);
		}
		if signals.grpc {
			annotations.insert(
				"nginx.ingress.kubernetes.io/backend-protocol".to_string(),
				"GRPC".to_string(),
			);
		}
	}

	if annotations.is_empty() {
		None
	} else {
		Some(annotations)
	}
}

/// Build a single `HTTPIngressPath` entry for the given path, service name, and port.
fn build_http_path(path: &str, service_name: &str, port: u16) -> HTTPIngressPath {
	HTTPIngressPath {
		path: Some(path.to_string()),
		path_type: "Prefix".to_string(),
		backend: IngressBackend {
			service: Some(IngressServiceBackend {
				name: service_name.to_string(),
				port: Some(ServiceBackendPort {
					number: Some(i32::from(port)),
					..Default::default()
				}),
			}),
			..Default::default()
		},
	}
}

pub(crate) fn build_ingress(
	app: &ReinhardtApp,
	routes: &[RouteMetadata],
	app_port: u16,
	host: Option<&str>,
	signals: Option<&InfraSignals>,
	pages_config: Option<&ResolvedPagesConfig>,
) -> Result<Option<Ingress>, Error> {
	let app_name = app.name_any();

	let mut paths: Vec<HTTPIngressPath> = routes
		.iter()
		.map(|route| build_http_path(&route.path, &app_name, app_port))
		.collect();

	// Append signal-driven paths if not already present in routes
	if let Some(signals) = signals {
		if signals.graphql && !paths.iter().any(|p| p.path.as_deref() == Some("/graphql/")) {
			paths.push(build_http_path("/graphql/", &app_name, app_port));
		}
		if signals.admin_panel && !paths.iter().any(|p| p.path.as_deref() == Some("/admin/")) {
			paths.push(build_http_path("/admin/", &app_name, app_port));
		}
	}

	// Insert static file path for pages sidecar before any catch-all `/` path
	if let Some(config) = pages_config
		&& !paths
			.iter()
			.any(|p| p.path.as_deref() == Some(config.static_url.as_str()))
	{
		let static_path = build_http_path(&config.static_url, &app_name, 8080);
		if let Some(root_idx) = paths.iter().position(|p| p.path.as_deref() == Some("/")) {
			paths.insert(root_idx, static_path);
		} else {
			paths.push(static_path);
		}
	}

	// Skip Ingress creation when there are no paths to route
	if paths.is_empty() {
		return Ok(None);
	}

	let labels = standard_labels(app, Component::Ingress);
	let namespace = super::require_namespace(app)?;
	let owner_ref = owner_reference(app)?;

	let rule = IngressRule {
		host: host.map(String::from),
		http: Some(HTTPIngressRuleValue { paths }),
	};

	let mut annotations = build_annotations(signals);
	if pages_config.is_some() {
		annotations.get_or_insert_with(BTreeMap::new).insert(
			"nginx.ingress.kubernetes.io/proxy-buffering".to_string(),
			"on".to_string(),
		);
	}

	Ok(Some(Ingress {
		metadata: ObjectMeta {
			name: Some(app_name),
			namespace: Some(namespace),
			labels: Some(labels),
			annotations,
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		spec: Some(IngressSpec {
			ingress_class_name: Some("nginx".to_string()),
			rules: Some(vec![rule]),
			..Default::default()
		}),
		..Default::default()
	}))
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
		let ingress = build_ingress(&app, &routes, 8000, None, None, None)
			.expect("build should succeed")
			.expect("ingress should be created for non-empty routes");

		// Assert
		assert_eq!(ingress.metadata.name.as_deref(), Some("my-app"));
	}

	#[rstest]
	fn test_build_ingress_paths_from_routes() {
		// Arrange
		let app = make_test_app("web");
		let routes = vec![make_route("/api/users/"), make_route("/api/posts/")];

		// Act
		let ingress = build_ingress(&app, &routes, 8080, None, None, None)
			.expect("build should succeed")
			.expect("ingress should be created for non-empty routes");

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
		let ingress = build_ingress(&app, &routes, 80, Some("example.com"), None, None)
			.expect("build should succeed")
			.expect("ingress should be created for non-empty routes");

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
		let ingress = build_ingress(&app, &routes, 80, None, None, None)
			.expect("build should succeed")
			.expect("ingress should be created for non-empty routes");

		// Assert
		let spec = ingress.spec.unwrap();
		let rules = spec.rules.as_ref().unwrap();
		assert!(rules[0].host.is_none());
	}

	#[rstest]
	fn test_build_ingress_empty_routes_returns_none() {
		// Arrange
		let app = make_test_app("web");
		let routes: Vec<RouteMetadata> = vec![];

		// Act
		let result =
			build_ingress(&app, &routes, 8000, None, None, None).expect("build should succeed");

		// Assert
		assert!(result.is_none());
	}

	#[rstest]
	fn test_build_ingress_websocket_annotations() {
		// Arrange
		let app = make_test_app("ws-app");
		let routes = vec![make_route("/ws/")];
		let signals = InfraSignals {
			websocket: true,
			..Default::default()
		};

		// Act
		let ingress = build_ingress(&app, &routes, 8000, None, Some(&signals), None)
			.expect("build should succeed")
			.expect("ingress should be created for non-empty routes");

		// Assert
		let annotations = ingress
			.metadata
			.annotations
			.expect("annotations should be set");
		assert_eq!(annotations.len(), 3);
		assert_eq!(
			annotations.get("nginx.ingress.kubernetes.io/proxy-read-timeout"),
			Some(&"3600".to_string()),
		);
		assert_eq!(
			annotations.get("nginx.ingress.kubernetes.io/proxy-send-timeout"),
			Some(&"3600".to_string()),
		);
		assert_eq!(
			annotations.get("nginx.ingress.kubernetes.io/upstream-hash-by"),
			Some(&"$remote_addr".to_string()),
		);
	}

	#[rstest]
	fn test_build_ingress_grpc_annotations() {
		// Arrange
		let app = make_test_app("grpc-app");
		let routes = vec![make_route("/grpc.Service/")];
		let signals = InfraSignals {
			grpc: true,
			..Default::default()
		};

		// Act
		let ingress = build_ingress(&app, &routes, 50051, None, Some(&signals), None)
			.expect("build should succeed")
			.expect("ingress should be created for non-empty routes");

		// Assert
		let annotations = ingress
			.metadata
			.annotations
			.expect("annotations should be set");
		assert_eq!(annotations.len(), 1);
		assert_eq!(
			annotations.get("nginx.ingress.kubernetes.io/backend-protocol"),
			Some(&"GRPC".to_string()),
		);
	}

	#[rstest]
	fn test_build_ingress_combined_annotations() {
		// Arrange
		let app = make_test_app("combo-app");
		let routes = vec![make_route("/")];
		let signals = InfraSignals {
			websocket: true,
			grpc: true,
			..Default::default()
		};

		// Act
		let ingress = build_ingress(&app, &routes, 8000, None, Some(&signals), None)
			.expect("build should succeed")
			.expect("ingress should be created for non-empty routes");

		// Assert
		let annotations = ingress
			.metadata
			.annotations
			.expect("annotations should be set");
		assert_eq!(annotations.len(), 4);
		assert!(annotations.contains_key("nginx.ingress.kubernetes.io/proxy-read-timeout"));
		assert!(annotations.contains_key("nginx.ingress.kubernetes.io/proxy-send-timeout"));
		assert!(annotations.contains_key("nginx.ingress.kubernetes.io/upstream-hash-by"));
		assert_eq!(
			annotations.get("nginx.ingress.kubernetes.io/backend-protocol"),
			Some(&"GRPC".to_string()),
		);
	}

	#[rstest]
	fn test_build_ingress_no_annotations_without_signals() {
		// Arrange
		let app = make_test_app("plain-app");
		let routes = vec![make_route("/api/")];

		// Act
		let ingress = build_ingress(&app, &routes, 8000, None, None, None)
			.expect("build should succeed")
			.expect("ingress should be created for non-empty routes");

		// Assert
		assert!(ingress.metadata.annotations.is_none());
	}

	#[rstest]
	fn test_build_ingress_graphql_path_added() {
		// Arrange
		let app = make_test_app("gql-app");
		let routes = vec![make_route("/api/")];
		let signals = InfraSignals {
			graphql: true,
			..Default::default()
		};

		// Act
		let ingress = build_ingress(&app, &routes, 8000, None, Some(&signals), None)
			.expect("build should succeed")
			.expect("ingress should be created for non-empty routes");

		// Assert
		let spec = ingress.spec.unwrap();
		let paths = &spec.rules.as_ref().unwrap()[0].http.as_ref().unwrap().paths;
		assert_eq!(paths.len(), 2);
		assert_eq!(paths[0].path.as_deref(), Some("/api/"));
		assert_eq!(paths[1].path.as_deref(), Some("/graphql/"));
	}

	#[rstest]
	fn test_build_ingress_admin_path_added() {
		// Arrange
		let app = make_test_app("admin-app");
		let routes = vec![make_route("/api/")];
		let signals = InfraSignals {
			admin_panel: true,
			..Default::default()
		};

		// Act
		let ingress = build_ingress(&app, &routes, 8000, None, Some(&signals), None)
			.expect("build should succeed")
			.expect("ingress should be created for non-empty routes");

		// Assert
		let spec = ingress.spec.unwrap();
		let paths = &spec.rules.as_ref().unwrap()[0].http.as_ref().unwrap().paths;
		assert_eq!(paths.len(), 2);
		assert_eq!(paths[0].path.as_deref(), Some("/api/"));
		assert_eq!(paths[1].path.as_deref(), Some("/admin/"));
	}

	#[rstest]
	fn test_build_ingress_no_duplicate_graphql_path() {
		// Arrange
		let app = make_test_app("gql-app");
		let routes = vec![make_route("/api/"), make_route("/graphql/")];
		let signals = InfraSignals {
			graphql: true,
			..Default::default()
		};

		// Act
		let ingress = build_ingress(&app, &routes, 8000, None, Some(&signals), None)
			.expect("build should succeed")
			.expect("ingress should be created for non-empty routes");

		// Assert
		let spec = ingress.spec.unwrap();
		let paths = &spec.rules.as_ref().unwrap()[0].http.as_ref().unwrap().paths;
		assert_eq!(paths.len(), 2);
		let graphql_count = paths
			.iter()
			.filter(|p| p.path.as_deref() == Some("/graphql/"))
			.count();
		assert_eq!(graphql_count, 1);
	}

	#[rstest]
	fn test_build_ingress_with_pages_adds_static_path() {
		// Arrange
		let app = make_test_app("app");
		let routes = vec![RouteMetadata {
			path: "/".to_string(),
			methods: vec!["GET".to_string()],
			name: None,
			namespace: None,
		}];
		let pages = crate::inference::pages::ResolvedPagesConfig::default();

		// Act
		let ingress = build_ingress(&app, &routes, 8000, None, None, Some(&pages))
			.unwrap()
			.unwrap();
		let rules = ingress.spec.unwrap().rules.unwrap();
		let paths = &rules[0].http.as_ref().unwrap().paths;

		// Assert
		let static_path = paths.iter().find(|p| p.path.as_deref() == Some("/static/"));
		assert!(static_path.is_some());
		let backend = &static_path.unwrap().backend;
		let svc_port = &backend.service.as_ref().unwrap().port.as_ref().unwrap();
		assert_eq!(svc_port.number, Some(8080));
	}

	#[rstest]
	fn test_build_ingress_with_pages_preserves_existing_paths() {
		// Arrange
		let app = make_test_app("app");
		let routes = vec![
			RouteMetadata {
				path: "/api/".to_string(),
				methods: vec!["GET".to_string()],
				name: None,
				namespace: None,
			},
			RouteMetadata {
				path: "/".to_string(),
				methods: vec!["GET".to_string()],
				name: None,
				namespace: None,
			},
		];
		let pages = crate::inference::pages::ResolvedPagesConfig::default();

		// Act
		let ingress = build_ingress(&app, &routes, 8000, None, None, Some(&pages))
			.unwrap()
			.unwrap();
		let spec = ingress.spec.unwrap();
		let rules = spec.rules.unwrap();
		let paths = &rules[0].http.as_ref().unwrap().paths;

		// Assert
		assert!(paths.iter().any(|p| p.path.as_deref() == Some("/static/")));
		assert!(paths.iter().any(|p| p.path.as_deref() == Some("/api/")));
		assert!(paths.iter().any(|p| p.path.as_deref() == Some("/")));
	}

	#[rstest]
	fn test_build_ingress_static_path_before_root() {
		// Arrange
		let app = make_test_app("app");
		let routes = vec![RouteMetadata {
			path: "/".to_string(),
			methods: vec!["GET".to_string()],
			name: None,
			namespace: None,
		}];
		let pages = crate::inference::pages::ResolvedPagesConfig::default();

		// Act
		let ingress = build_ingress(&app, &routes, 8000, None, None, Some(&pages))
			.unwrap()
			.unwrap();
		let spec = ingress.spec.unwrap();
		let rules = spec.rules.unwrap();
		let paths = &rules[0].http.as_ref().unwrap().paths;

		// Assert — /static/ appears before /
		let static_idx = paths
			.iter()
			.position(|p| p.path.as_deref() == Some("/static/"))
			.unwrap();
		let root_idx = paths
			.iter()
			.position(|p| p.path.as_deref() == Some("/"))
			.unwrap();
		assert!(static_idx < root_idx);
	}
}
