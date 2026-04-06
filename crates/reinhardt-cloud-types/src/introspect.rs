//! Types mirroring reinhardt-web's `manage introspect` output.
//!
//! These types represent the introspection data produced by a reinhardt-web
//! application, used by the zero-config inference engine to derive Kubernetes
//! resource configurations automatically.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Top-level output of the `manage introspect` command.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct IntrospectOutput {
	/// Application metadata (name, version).
	#[serde(default)]
	pub app: AppMetadata,
	/// Database configurations discovered by introspection.
	#[serde(default)]
	pub databases: Vec<DatabaseMetadata>,
	/// URL routes registered in the application.
	#[serde(default)]
	pub routes: Vec<RouteMetadata>,
	/// Middleware stack configured in the application.
	#[serde(default)]
	pub middleware: Vec<MiddlewareMetadata>,
	/// Application settings (server, security).
	#[serde(default)]
	pub settings: SettingsMetadata,
	/// Feature declarations and infrastructure signals.
	#[serde(default)]
	pub features: FeaturesMetadata,
}

/// Basic application identity.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct AppMetadata {
	/// Application name.
	#[serde(default)]
	pub name: String,
	/// Application version string.
	#[serde(default)]
	pub version: String,
}

/// A single database backend discovered by introspection.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct DatabaseMetadata {
	/// The alias used in settings (e.g., `"default"`).
	#[serde(default)]
	pub alias: String,
	/// Database engine identifier (e.g., `"postgres"`).
	#[serde(default)]
	pub engine: String,
	/// Tables registered through this database backend.
	#[serde(default)]
	pub tables: Vec<TableMetadata>,
}

/// A database table discovered through introspection.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct TableMetadata {
	/// Table name in the database.
	#[serde(default)]
	pub name: String,
	/// The application that owns this table.
	#[serde(default)]
	pub app: String,
}

/// A URL route registered in the application.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct RouteMetadata {
	/// URL path pattern (e.g., `/api/users/`).
	#[serde(default)]
	pub path: String,
	/// HTTP methods accepted by this route.
	#[serde(default)]
	pub methods: Vec<String>,
	/// Optional route name for reverse-URL lookups.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub name: Option<String>,
	/// Optional namespace grouping.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub namespace: Option<String>,
}

/// A middleware entry in the application stack.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct MiddlewareMetadata {
	/// Short name of the middleware.
	#[serde(default)]
	pub name: String,
	/// Fully-qualified type name.
	#[serde(default)]
	pub type_name: String,
}

/// Aggregated application settings relevant to deployment.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct SettingsMetadata {
	/// Server configuration.
	#[serde(default)]
	pub server: ServerSettings,
	/// Security-related settings.
	#[serde(default)]
	pub security: SecuritySettings,
}

/// Server-level settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServerSettings {
	/// Default listening port (defaults to 8000).
	#[serde(default = "default_port")]
	pub default_port: u16,
	/// Whether the application runs in debug mode.
	#[serde(default)]
	pub debug: bool,
}

/// Returns the default server port (8000).
fn default_port() -> u16 {
	8000
}

impl Default for ServerSettings {
	fn default() -> Self {
		Self {
			default_port: default_port(),
			debug: false,
		}
	}
}

/// Security-related deployment settings.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct SecuritySettings {
	/// Whether HTTP-to-HTTPS redirect is enabled.
	#[serde(default)]
	pub ssl_redirect: bool,
	/// Whether the session cookie is marked Secure.
	#[serde(default)]
	pub session_cookie_secure: bool,
	/// Whether the CSRF cookie is marked Secure.
	#[serde(default)]
	pub csrf_cookie_secure: bool,
	/// Whether HSTS (HTTP Strict Transport Security) is enabled.
	#[serde(default)]
	pub hsts_enabled: bool,
}

/// Feature declarations and resolved feature set.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct FeaturesMetadata {
	/// Features explicitly declared by the application.
	#[serde(default)]
	pub declared: Vec<String>,
	/// Features resolved after dependency analysis.
	#[serde(default)]
	pub resolved: Vec<String>,
	/// Infrastructure signals inferred from application code.
	#[serde(default)]
	pub infrastructure_signals: InfraSignals,
}

/// Infrastructure signals detected during introspection.
///
/// Each field indicates whether the application uses a particular
/// infrastructure component and, where applicable, which backend.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct InfraSignals {
	/// Database backend identifier (e.g., `"postgres"`).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub database: Option<String>,
	/// Cache backend identifier (e.g., `"redis"`).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub cache: Option<String>,
	/// Whether the application uses WebSocket connections.
	#[serde(default)]
	pub websocket: bool,
	/// Whether the application uses background workers.
	#[serde(default)]
	pub background_worker: bool,
	/// Whether the application exposes gRPC services.
	#[serde(default)]
	pub grpc: bool,
	/// Object storage backend identifier (e.g., `"s3"`).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub storage: Option<String>,
	/// Mail backend identifier (e.g., `"smtp"`).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub mail: Option<String>,
	/// Session backend identifier (e.g., `"redis"`, `"db"`).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub session_backend: Option<String>,
	/// Whether the application uses GraphQL.
	#[serde(default)]
	pub graphql: bool,
	/// Whether the application includes an admin panel.
	#[serde(default)]
	pub admin_panel: bool,
	/// Whether internationalization (i18n) is enabled.
	#[serde(default)]
	pub i18n: bool,
	/// Whether the application uses reinhardt-pages (WASM frontend).
	#[serde(default)]
	pub pages: bool,
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_introspect_output_yaml_roundtrip() {
		// Arrange
		let output = IntrospectOutput {
			app: AppMetadata {
				name: "my-app".to_string(),
				version: "1.0.0".to_string(),
			},
			databases: vec![DatabaseMetadata {
				alias: "default".to_string(),
				engine: "postgres".to_string(),
				tables: vec![TableMetadata {
					name: "users".to_string(),
					app: "auth".to_string(),
				}],
			}],
			routes: vec![RouteMetadata {
				path: "/api/users/".to_string(),
				methods: vec!["GET".to_string(), "POST".to_string()],
				name: Some("user-list".to_string()),
				namespace: Some("api".to_string()),
			}],
			middleware: vec![MiddlewareMetadata {
				name: "auth".to_string(),
				type_name: "AuthMiddleware".to_string(),
			}],
			settings: SettingsMetadata {
				server: ServerSettings {
					default_port: 8080,
					debug: true,
				},
				security: SecuritySettings {
					ssl_redirect: true,
					session_cookie_secure: true,
					csrf_cookie_secure: true,
					hsts_enabled: true,
				},
			},
			features: FeaturesMetadata {
				declared: vec!["database".to_string()],
				resolved: vec!["database".to_string(), "auth".to_string()],
				infrastructure_signals: InfraSignals {
					database: Some("postgres".to_string()),
					cache: Some("redis".to_string()),
					websocket: true,
					background_worker: false,
					grpc: false,
					storage: None,
					mail: Some("smtp".to_string()),
					session_backend: Some("redis".to_string()),
					graphql: false,
					admin_panel: true,
					i18n: false,
					pages: false,
				},
			},
		};

		// Act
		let yaml = serde_yaml::to_string(&output).expect("serialize to YAML");
		let deserialized: IntrospectOutput =
			serde_yaml::from_str(&yaml).expect("deserialize from YAML");

		// Assert
		assert_eq!(deserialized.app.name, "my-app");
		assert_eq!(deserialized.app.version, "1.0.0");
		assert_eq!(deserialized.databases.len(), 1);
		assert_eq!(deserialized.databases[0].alias, "default");
		assert_eq!(deserialized.databases[0].engine, "postgres");
		assert_eq!(deserialized.databases[0].tables.len(), 1);
		assert_eq!(deserialized.databases[0].tables[0].name, "users");
		assert_eq!(deserialized.routes.len(), 1);
		assert_eq!(deserialized.routes[0].path, "/api/users/");
		assert_eq!(deserialized.routes[0].methods, vec!["GET", "POST"]);
		assert_eq!(deserialized.routes[0].name, Some("user-list".to_string()));
		assert_eq!(deserialized.middleware.len(), 1);
		assert_eq!(deserialized.settings.server.default_port, 8080);
		assert!(deserialized.settings.server.debug);
		assert!(deserialized.settings.security.ssl_redirect);
		assert_eq!(deserialized.features.declared, vec!["database"]);
		assert_eq!(
			deserialized.features.infrastructure_signals.database,
			Some("postgres".to_string())
		);
		assert_eq!(
			deserialized.features.infrastructure_signals.cache,
			Some("redis".to_string())
		);
		assert!(deserialized.features.infrastructure_signals.websocket);
		assert!(
			deserialized.features.infrastructure_signals.admin_panel
		);
	}

	#[rstest]
	fn test_introspect_output_partial_yaml() {
		// Arrange
		let yaml = r#"
app:
  name: "partial-app"
databases: []
"#;

		// Act
		let output: IntrospectOutput =
			serde_yaml::from_str(yaml).expect("deserialize partial YAML");

		// Assert
		assert_eq!(output.app.name, "partial-app");
		assert_eq!(output.app.version, "");
		assert!(output.databases.is_empty());
		assert!(output.routes.is_empty());
		assert!(output.middleware.is_empty());
		assert_eq!(output.settings.server.default_port, 8000);
		assert!(!output.settings.server.debug);
		assert!(!output.settings.security.ssl_redirect);
		assert!(output.features.declared.is_empty());
		assert!(output.features.resolved.is_empty());
		assert_eq!(output.features.infrastructure_signals.database, None);
		assert!(!output.features.infrastructure_signals.websocket);
	}

	#[rstest]
	fn test_introspect_output_empty_yaml() {
		// Arrange
		let yaml = "{}";

		// Act
		let output: IntrospectOutput = serde_yaml::from_str(yaml).expect("deserialize empty YAML");

		// Assert
		assert_eq!(output.app.name, "");
		assert_eq!(output.app.version, "");
		assert!(output.databases.is_empty());
		assert!(output.routes.is_empty());
		assert!(output.middleware.is_empty());
		assert_eq!(output.settings.server.default_port, 8000);
		assert!(!output.settings.server.debug);
		assert!(!output.settings.security.ssl_redirect);
		assert!(!output.settings.security.session_cookie_secure);
		assert!(!output.settings.security.csrf_cookie_secure);
		assert!(!output.settings.security.hsts_enabled);
		assert!(output.features.declared.is_empty());
		assert!(output.features.resolved.is_empty());
		assert_eq!(output.features.infrastructure_signals.database, None);
		assert_eq!(output.features.infrastructure_signals.cache, None);
		assert!(!output.features.infrastructure_signals.websocket);
		assert!(
			!output.features.infrastructure_signals.background_worker
		);
		assert!(!output.features.infrastructure_signals.grpc);
		assert_eq!(output.features.infrastructure_signals.storage, None);
		assert_eq!(output.features.infrastructure_signals.mail, None);
		assert_eq!(output.features.infrastructure_signals.session_backend, None);
		assert!(!output.features.infrastructure_signals.graphql);
		assert!(!output.features.infrastructure_signals.admin_panel);
		assert!(!output.features.infrastructure_signals.i18n);
		assert!(!output.features.infrastructure_signals.pages);
	}

	#[rstest]
	fn test_infra_signals_pages_field() {
		// Arrange
		let yaml = r#"
infrastructure_signals:
  pages: true
  database: postgres
"#;

		// Act
		let features: FeaturesMetadata = serde_yaml::from_str(yaml).unwrap();

		// Assert
		assert!(features.infrastructure_signals.pages);
		assert_eq!(
			features.infrastructure_signals.database.as_deref(),
			Some("postgres")
		);
	}

	#[rstest]
	fn test_infra_signals_pages_defaults_false() {
		// Arrange
		let yaml = "infrastructure_signals: {}";

		// Act
		let features: FeaturesMetadata = serde_yaml::from_str(yaml).unwrap();

		// Assert
		assert!(!features.infrastructure_signals.pages);
	}
}
