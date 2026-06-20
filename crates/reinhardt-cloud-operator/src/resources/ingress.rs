//! Ingress builder for operator-managed `Project` resources.

use std::collections::BTreeMap;

use k8s_openapi::api::networking::v1::{
	HTTPIngressPath, HTTPIngressRuleValue, Ingress, IngressBackend, IngressRule,
	IngressServiceBackend, IngressSpec, IngressTLS, ServiceBackendPort,
};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use reinhardt_cloud_types::crd::Project;
use reinhardt_cloud_types::introspect::{InfraSignals, RouteMetadata};

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;
use crate::inference::pages::ResolvedPagesConfig;

/// Builds an `Ingress` for the given `Project` from introspected route metadata.
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

fn resolve_tls(app: &Project, host: Option<&str>) -> Option<IngressTLS> {
	let tls = app.spec.services.as_ref()?.tls.as_ref()?;
	if !tls.enabled {
		return None;
	}

	let host = host.or_else(|| app.spec.services.as_ref()?.ingress_host.as_deref())?;
	let secret_name = tls.secret_name.clone()?;

	Some(IngressTLS {
		hosts: Some(vec![host.to_string()]),
		secret_name: Some(secret_name),
	})
}

fn merge_tls_annotations(app: &Project, annotations: &mut BTreeMap<String, String>) {
	let Some(tls) = app.spec.services.as_ref().and_then(|s| s.tls.as_ref()) else {
		return;
	};

	if !tls.enabled {
		return;
	}

	if let Some(issuer) = &tls.issuer {
		annotations.insert("cert-manager.io/issuer".to_string(), issuer.clone());
	}
}

pub(crate) fn build_ingress(
	app: &Project,
	routes: &[RouteMetadata],
	app_port: u16,
	host: Option<&str>,
	signals: Option<&InfraSignals>,
	pages_config: Option<&ResolvedPagesConfig>,
) -> Result<Option<Ingress>, Error> {
	let project_name = app.name_any();

	let mut paths: Vec<HTTPIngressPath> = routes
		.iter()
		.map(|route| build_http_path(&route.path, &project_name, app_port))
		.collect();

	// Append signal-driven paths if not already present in routes
	if let Some(signals) = signals {
		if signals.graphql && !paths.iter().any(|p| p.path.as_deref() == Some("/graphql/")) {
			paths.push(build_http_path("/graphql/", &project_name, app_port));
		}
		if signals.admin_panel && !paths.iter().any(|p| p.path.as_deref() == Some("/admin/")) {
			paths.push(build_http_path("/admin/", &project_name, app_port));
		}
	}

	// Insert static file path for pages sidecar before any catch-all `/` path
	if let Some(config) = pages_config
		&& !paths
			.iter()
			.any(|p| p.path.as_deref() == Some(config.static_url.as_str()))
	{
		let static_path = build_http_path(&config.static_url, &project_name, 8080);
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

	let mut annotations = build_annotations(signals).unwrap_or_default();
	merge_tls_annotations(app, &mut annotations);
	if pages_config.is_some() {
		annotations.insert(
			"nginx.ingress.kubernetes.io/proxy-buffering".to_string(),
			"on".to_string(),
		);
	}
	// Preview Projects carry the `reinhardt.dev/preview=true` label. Give such
	// Projects a cert-manager TLS section plus the per-namespace Issuer
	// annotation so cert-manager issues a certificate for the preview host.
	let is_preview = app
		.metadata
		.labels
		.as_ref()
		.and_then(|labels| labels.get("reinhardt.dev/preview"))
		.is_some_and(|value| value == "true");
	let tls = if is_preview && host.is_some() {
		annotations.insert(
			"cert-manager.io/issuer".to_string(),
			crate::resources::preview_namespace::ISSUER_NAME.to_string(),
		);
		Some(vec![IngressTLS {
			hosts: Some(vec![host.map(|h| h.to_string()).unwrap_or_default()]),
			secret_name: Some(format!("{project_name}-tls")),
		}])
	} else {
		resolve_tls(app, host).map(|tls| vec![tls])
	};
	let annotations = if annotations.is_empty() {
		None
	} else {
		Some(annotations)
	};

	Ok(Some(Ingress {
		metadata: ObjectMeta {
			name: Some(project_name),
			namespace: Some(namespace),
			labels: Some(labels),
			annotations,
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		spec: Some(IngressSpec {
			ingress_class_name: Some("nginx".to_string()),
			rules: Some(vec![rule]),
			tls,
			..Default::default()
		}),
		..Default::default()
	}))
}

#[cfg(test)]
mod tests {
	use super::*;
	use kube::api::ObjectMeta;
	use reinhardt_cloud_types::crd::ProjectSpec;
	use reinhardt_cloud_types::crd::spec::{ServiceTlsSpec, ServicesSpec};
	use rstest::rstest;

	fn make_test_app(name: &str) -> Project {
		Project {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				..Default::default()
			},
			spec: ProjectSpec {
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
	fn test_build_ingress_adds_tls_for_preview_with_host() {
		// Arrange — a preview-labeled Project with an ingress host.
		let mut app = make_test_app("my-app-pr-42");
		app.metadata.labels = Some(
			[("reinhardt.dev/preview".to_string(), "true".to_string())]
				.into_iter()
				.collect(),
		);
		let routes = vec![make_route("/")];

		// Act
		let ingress = build_ingress(
			&app,
			&routes,
			8080,
			Some("my-app-pr-42.preview.example.com"),
			None,
			None,
		)
		.expect("build should succeed")
		.expect("ingress should be created");

		// Assert — TLS section targets the host and the cert-manager annotation
		// points at the preview Issuer.
		let spec = ingress.spec.expect("spec");
		let tls = spec.tls.expect("preview ingress must carry TLS");
		assert_eq!(tls.len(), 1);
		assert_eq!(
			tls[0].hosts.as_ref().unwrap()[0],
			"my-app-pr-42.preview.example.com"
		);
		assert_eq!(
			ingress
				.metadata
				.annotations
				.as_ref()
				.unwrap()
				.get("cert-manager.io/issuer")
				.map(String::as_str),
			Some(crate::resources::preview_namespace::ISSUER_NAME)
		);
	}

	#[rstest]
	fn test_build_ingress_omits_tls_for_non_preview() {
		// Arrange — a regular (non-preview) Project with a host.
		let app = make_test_app("web");
		let routes = vec![make_route("/")];

		// Act
		let ingress = build_ingress(&app, &routes, 80, Some("example.com"), None, None)
			.expect("build should succeed")
			.expect("ingress should be created");

		// Assert — no TLS section and no cert-manager annotation for non-previews.
		let spec = ingress.spec.expect("spec");
		assert!(spec.tls.is_none(), "non-preview ingress must not carry TLS");
		assert!(
			ingress
				.metadata
				.annotations
				.as_ref()
				.and_then(|annotations| annotations.get("cert-manager.io/issuer"))
				.is_none(),
			"non-preview ingress must not carry a cert-manager annotation"
		);
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
	fn test_build_ingress_with_tls_adds_tls_spec() {
		// Arrange
		let mut app = make_test_app("web");
		app.spec.services = Some(ServicesSpec {
			port: Some(80),
			target_port: Some(8000),
			ingress_host: Some("app.example.com".to_string()),
			tls: Some(ServiceTlsSpec {
				enabled: true,
				secret_name: Some("app-example-com-tls".to_string()),
				issuer: None,
				cluster_issuer: None,
			}),
		});
		let routes = vec![make_route("/")];

		// Act
		let ingress = build_ingress(&app, &routes, 80, Some("app.example.com"), None, None)
			.expect("build should succeed")
			.expect("ingress should be created");

		// Assert
		let spec = ingress.spec.expect("spec");
		let tls = spec.tls.expect("tls");
		assert_eq!(tls.len(), 1);
		assert_eq!(
			tls[0].hosts.as_ref().unwrap(),
			&vec!["app.example.com".to_string()]
		);
		assert_eq!(tls[0].secret_name.as_deref(), Some("app-example-com-tls"));
	}

	#[rstest]
	fn test_build_ingress_ignores_cluster_issuer_annotation() {
		// Arrange
		let mut app = make_test_app("web");
		app.spec.services = Some(ServicesSpec {
			port: Some(80),
			target_port: Some(8000),
			ingress_host: Some("app.example.com".to_string()),
			tls: Some(ServiceTlsSpec {
				enabled: true,
				secret_name: Some("app-example-com-tls".to_string()),
				issuer: None,
				cluster_issuer: Some("letsencrypt-prod".to_string()),
			}),
		});
		let routes = vec![make_route("/")];

		// Act
		let ingress = build_ingress(&app, &routes, 80, Some("app.example.com"), None, None)
			.expect("build should succeed")
			.expect("ingress should be created");

		// Assert
		assert_eq!(ingress.metadata.annotations, None);
	}

	#[rstest]
	fn test_build_ingress_with_namespace_issuer_annotation() {
		// Arrange
		let mut app = make_test_app("web");
		app.spec.services = Some(ServicesSpec {
			port: Some(80),
			target_port: Some(8000),
			ingress_host: Some("app.example.com".to_string()),
			tls: Some(ServiceTlsSpec {
				enabled: true,
				secret_name: Some("app-example-com-tls".to_string()),
				issuer: Some("letsencrypt-ns".to_string()),
				cluster_issuer: None,
			}),
		});
		let routes = vec![make_route("/")];

		// Act
		let ingress = build_ingress(&app, &routes, 80, Some("app.example.com"), None, None)
			.expect("build should succeed")
			.expect("ingress should be created");

		// Assert
		let annotations = ingress.metadata.annotations.expect("annotations");
		assert_eq!(
			annotations.get("cert-manager.io/issuer"),
			Some(&"letsencrypt-ns".to_string())
		);
		assert_eq!(annotations.get("cert-manager.io/cluster-issuer"), None);
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

	#[rstest]
	fn test_build_ingress_explicit_host_with_pages_adds_static_path() {
		// Arrange — simulates #97: explicit ingress_host + pages enabled
		let app = make_test_app("pages-app");
		let routes = vec![RouteMetadata {
			path: "/".to_string(),
			methods: vec![],
			name: None,
			namespace: None,
		}];
		let pages = crate::inference::pages::ResolvedPagesConfig::default();

		// Act
		let ingress = build_ingress(
			&app,
			&routes,
			8080,
			Some("my-app.example.com"),
			None,
			Some(&pages),
		)
		.unwrap()
		.unwrap();
		let spec = ingress.spec.unwrap();
		let rules = spec.rules.unwrap();
		let paths = &rules[0].http.as_ref().unwrap().paths;

		// Assert — host is set and /static/ path is present
		assert_eq!(rules[0].host.as_deref(), Some("my-app.example.com"));
		assert!(paths.iter().any(|p| p.path.as_deref() == Some("/static/")));
		let static_path = paths
			.iter()
			.find(|p| p.path.as_deref() == Some("/static/"))
			.unwrap();
		let svc_port = &static_path
			.backend
			.service
			.as_ref()
			.unwrap()
			.port
			.as_ref()
			.unwrap();
		assert_eq!(svc_port.number, Some(8080));
	}
}
