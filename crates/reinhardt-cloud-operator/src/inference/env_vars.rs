//! Environment variable auto-injection and merging for deployed applications.
//!
//! Provides builders for database connection strings, system variables, and
//! a priority-based merge function where user overrides always win.

use std::collections::{BTreeMap, HashSet};

use k8s_openapi::api::core::v1::{EnvVar, EnvVarSource, SecretKeySelector};
use kube::ResourceExt;
use reinhardt_cloud_types::crd::ReinhardtApp;

use super::platform::Platform;

/// Build database connection environment variables that reference a
/// Kubernetes `Secret` for the password, keeping the sensitive value out
/// of the pod spec.
///
/// Non-secret values (host, port, db name, user) are derived from
/// conventions shared with `infer_database_resources`:
/// - Secret name: `{app_name}-db-credentials` (keys `username` / `password`)
/// - Host / port / db name / user: derived from `db_host` and the naming
///   convention used by the inference module. These are deterministic
///   identifiers (not credentials), so embedding them as plain values is safe.
///
/// The caller is responsible for ensuring a PostgreSQL database will be
/// provisioned for this app and that the corresponding credentials Secret
/// (`{app_name}-db-credentials`) will exist.
///
/// The `db_host` parameter must be the DNS name of the Service that exposes
/// the database:
/// - For the explicit `spec.database` path: `"{app_name}-db"` (the headless
///   Service created by `infer_database_resources`).
/// - For the introspect-provisioned path: `"{app_name}-postgresql"` (the
///   Service created by `reconcile_db_service_resource`).
pub(crate) fn build_database_env_vars_from_secret(
	app: &ReinhardtApp,
	platform: &Platform,
	db_host: &str,
) -> Vec<EnvVar> {
	let app_name = app.name_any();
	let secret_name = format!("{app_name}-db-credentials");

	// Port follows the standard PostgreSQL convention. For cloud platforms
	// (AWS RDS, GCP CloudSQL), the db_host must be supplied by the caller
	// based on the managed instance endpoint; until cloud endpoint resolution
	// is implemented, the host should be overridden via `spec.env.DATABASE_URL`.
	//
	// NOTE: For Platform::Aws and Platform::Gcp, cloud-managed database
	// endpoints are not yet resolved automatically. The db_host passed here
	// is a best-effort placeholder. Apps on cloud platforms that require
	// the correct RDS/CloudSQL endpoint should override DATABASE_URL via
	// `spec.env` until automatic cloud endpoint resolution is implemented.
	let port = match platform {
		Platform::Onpremise => 5432,
		Platform::Aws | Platform::Gcp => {
			tracing::warn!(
				app = %app_name,
				"Cloud-managed database endpoint resolution is not yet implemented. \
				 DATABASE_URL and REINHARDT_DATABASE_HOST will use the placeholder host \
				 '{}'. Override via spec.env.DATABASE_URL until cloud endpoint \
				 resolution is supported.",
				db_host
			);
			5432
		}
	};
	let host = db_host.to_string();

	// Both identifiers use replace('-', "_") to produce valid SQL identifiers,
	// matching the convention used by infer_database_resources in inference/database.rs.
	let sanitized_name = app_name.replace('-', "_");
	let db_name = format!("{sanitized_name}_db");
	let db_user = sanitized_name;

	// DATABASE_URL provides a single-connection-string alternative to the
	// individual REINHARDT_DATABASE_* vars. The password is embedded via
	// $(REINHARDT_DATABASE_PASSWORD), which is resolved by the kubelet at
	// pod start after all env vars are evaluated.
	let database_url =
		format!("postgres://{db_user}:$(REINHARDT_DATABASE_PASSWORD)@{host}:{port}/{db_name}");

	vec![
		env_var("REINHARDT_DATABASE_HOST", &host),
		env_var("REINHARDT_DATABASE_PORT", &port.to_string()),
		env_var("REINHARDT_DATABASE_NAME", &db_name),
		env_var("REINHARDT_DATABASE_USER", &db_user),
		EnvVar {
			name: "REINHARDT_DATABASE_PASSWORD".to_string(),
			value: None,
			value_from: Some(EnvVarSource {
				secret_key_ref: Some(SecretKeySelector {
					name: secret_name.clone(),
					key: "password".to_string(),
					optional: Some(false),
				}),
				..Default::default()
			}),
		},
		env_var("DATABASE_URL", &database_url),
	]
}

/// Build OpenTelemetry environment variables for an application Pod.
///
/// Propagates the active trace context into the Pod so that application spans
/// are correlated with the operator's reconcile span. Also sets standard OTel
/// configuration variables so the Pod's SDK picks up the correct exporter and
/// service identity without requiring manual configuration.
///
/// * `app_name` — value for `OTEL_SERVICE_NAME` (typically the app's name).
///
/// Variables injected:
/// * `TRACEPARENT` — W3C `traceparent` of the current reconcile span (omitted
///   when the current span context is not valid). Note: OTel SDKs do not read
///   `TRACEPARENT` automatically; the application must bootstrap context by
///   reading this variable and explicitly setting it as the parent for the
///   process's root span.
/// * `OTEL_PROPAGATORS` — fixed to `tracecontext`.
/// * `OTEL_SERVICE_NAME` — set to `app_name`.
/// * `OTEL_EXPORTER_OTLP_ENDPOINT` — forwarded from the operator's own
///   environment variable of the same name when present.
pub(crate) fn build_otel_env_vars(app_name: &str) -> Vec<EnvVar> {
	use opentelemetry::trace::TraceContextExt;
	use tracing::Span;
	use tracing_opentelemetry::OpenTelemetrySpanExt;

	let mut vars = vec![
		env_var("OTEL_PROPAGATORS", "tracecontext"),
		env_var("OTEL_SERVICE_NAME", app_name),
	];

	// Inject the operator's OTLP endpoint so the Pod sends spans to the same
	// collector without requiring user-level configuration.
	if let Ok(endpoint) = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
		&& !endpoint.is_empty()
	{
		vars.push(env_var("OTEL_EXPORTER_OTLP_ENDPOINT", &endpoint));
	}

	// Propagate the current reconcile span's traceparent so that application
	// spans are nested inside the operator's reconcile trace.
	let otel_cx = Span::current().context();
	let otel_span = otel_cx.span();
	if otel_span.span_context().is_valid()
		&& let Some(tp) = reinhardt_cloud_telemetry::traceparent_from_context(&otel_cx)
	{
		vars.push(env_var("TRACEPARENT", &tp));
	}

	vars
}

/// Build system environment variables that are always injected.
pub(crate) fn build_system_env_vars() -> Vec<EnvVar> {
	vec![
		env_var("REINHARDT_ENV", "production"),
		env_var(
			"REINHARDT_CLOUD_CONFIG_DIR",
			"/etc/reinhardt-cloud/settings",
		),
	]
}

/// Merge auto-generated and user-supplied environment variables.
///
/// User overrides (`user_vars`) always take priority over auto-generated
/// variables (`auto_vars`). When both define the same key, the user value
/// is kept and the auto-generated value is discarded.
pub(crate) fn merge_env_vars(
	auto_vars: &[EnvVar],
	user_vars: &BTreeMap<String, String>,
) -> Vec<EnvVar> {
	let mut result: Vec<EnvVar> = Vec::new();
	let mut seen = HashSet::new();

	// User vars first (highest priority)
	for (k, v) in user_vars {
		result.push(env_var(k, v));
		seen.insert(k.clone());
	}

	// Auto vars only if not overridden by user
	for var in auto_vars {
		if !seen.contains(&var.name) {
			result.push(var.clone());
			seen.insert(var.name.clone());
		}
	}

	result
}

fn env_var(name: &str, value: &str) -> EnvVar {
	EnvVar {
		name: name.to_string(),
		value: Some(value.to_string()),
		..Default::default()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
	use reinhardt_cloud_types::crd::ReinhardtAppSpec;
	use reinhardt_cloud_types::crd::database::{DatabaseEngine, DatabaseSpec};
	use rstest::rstest;

	fn make_app_with_db(name: &str) -> ReinhardtApp {
		ReinhardtApp {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid".to_string()),
				..Default::default()
			},
			spec: ReinhardtAppSpec {
				image: "app:latest".to_string(),
				database: Some(DatabaseSpec {
					engine: DatabaseEngine::Postgresql,
					instance_class: None,
					storage_gb: None,
					version: None,
				}),
				..Default::default()
			},
			status: None,
		}
	}

	#[rstest]
	fn build_database_env_vars_from_secret_references_credentials_secret() {
		// Arrange
		let app = make_app_with_db("myapp");

		// Act
		let vars = build_database_env_vars_from_secret(&app, &Platform::Onpremise, "myapp-db");

		// Assert — password is injected via SecretKeyRef, never inlined
		let password_var = vars
			.iter()
			.find(|v| v.name == "REINHARDT_DATABASE_PASSWORD")
			.expect("password env var must be present");
		assert!(
			password_var.value.is_none(),
			"password value must be empty; it must come from the Secret"
		);
		let key_ref = password_var
			.value_from
			.as_ref()
			.and_then(|vf| vf.secret_key_ref.as_ref())
			.expect("password must reference a SecretKeyRef");
		assert_eq!(key_ref.name, "myapp-db-credentials");
		assert_eq!(key_ref.key, "password");
	}

	#[rstest]
	fn build_database_env_vars_from_secret_sets_plain_metadata_fields() {
		// Arrange
		let app = make_app_with_db("my-app");

		// Act
		let vars = build_database_env_vars_from_secret(&app, &Platform::Onpremise, "my-app-db");

		// Assert — non-secret identifiers are safe as plain values
		let host = vars
			.iter()
			.find(|v| v.name == "REINHARDT_DATABASE_HOST")
			.unwrap();
		assert_eq!(host.value.as_deref(), Some("my-app-db"));

		let port = vars
			.iter()
			.find(|v| v.name == "REINHARDT_DATABASE_PORT")
			.unwrap();
		assert_eq!(port.value.as_deref(), Some("5432"));

		let db_name = vars
			.iter()
			.find(|v| v.name == "REINHARDT_DATABASE_NAME")
			.unwrap();
		assert_eq!(db_name.value.as_deref(), Some("my_app_db"));

		let user = vars
			.iter()
			.find(|v| v.name == "REINHARDT_DATABASE_USER")
			.unwrap();
		assert_eq!(user.value.as_deref(), Some("my_app"));
	}

	#[rstest]
	fn build_database_env_vars_from_secret_includes_database_url() {
		// Arrange
		let app = make_app_with_db("my-app");

		// Act
		let vars = build_database_env_vars_from_secret(&app, &Platform::Onpremise, "my-app-db");

		// Assert — DATABASE_URL must be present and contain the connection params
		let url_var = vars
			.iter()
			.find(|v| v.name == "DATABASE_URL")
			.expect("DATABASE_URL env var must be present");
		let url = url_var
			.value
			.as_deref()
			.expect("DATABASE_URL must have a value");
		assert!(
			url.starts_with("postgres://"),
			"DATABASE_URL must use postgres:// scheme"
		);
		assert!(
			url.contains("my-app-db"),
			"DATABASE_URL must include the host"
		);
		assert!(
			url.contains("my_app_db"),
			"DATABASE_URL must include the db name"
		);
	}

	#[rstest]
	fn build_system_env_vars_contains_required_keys() {
		// Arrange & Act
		let vars = build_system_env_vars();

		// Assert
		assert_eq!(vars.len(), 2);

		let env_var = vars.iter().find(|v| v.name == "REINHARDT_ENV").unwrap();
		assert_eq!(env_var.value.as_deref(), Some("production"));

		let config_var = vars
			.iter()
			.find(|v| v.name == "REINHARDT_CLOUD_CONFIG_DIR")
			.unwrap();
		assert_eq!(
			config_var.value.as_deref(),
			Some("/etc/reinhardt-cloud/settings")
		);
	}

	#[rstest]
	fn merge_env_vars_user_overrides_auto_vars() {
		// Arrange
		let auto_vars = vec![
			env_var("DATABASE_URL", "auto-url"),
			env_var("REINHARDT_ENV", "production"),
		];
		let user_vars = BTreeMap::from([("DATABASE_URL".to_string(), "custom-url".to_string())]);

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		let db_var = merged.iter().find(|v| v.name == "DATABASE_URL").unwrap();
		assert_eq!(db_var.value.as_deref(), Some("custom-url"));

		// Auto var not overridden is preserved
		let env_var = merged.iter().find(|v| v.name == "REINHARDT_ENV").unwrap();
		assert_eq!(env_var.value.as_deref(), Some("production"));
	}

	#[rstest]
	fn merge_env_vars_preserves_all_unique_keys() {
		// Arrange
		let auto_vars = vec![env_var("AUTO_KEY", "auto_val")];
		let user_vars = BTreeMap::from([("USER_KEY".to_string(), "user_val".to_string())]);

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		assert_eq!(merged.len(), 2);
		assert!(merged.iter().any(|v| v.name == "AUTO_KEY"));
		assert!(merged.iter().any(|v| v.name == "USER_KEY"));
	}

	#[rstest]
	fn merge_env_vars_no_duplicates() {
		// Arrange
		let auto_vars = vec![env_var("KEY_A", "auto_a"), env_var("KEY_B", "auto_b")];
		let user_vars = BTreeMap::from([
			("KEY_A".to_string(), "user_a".to_string()),
			("KEY_C".to_string(), "user_c".to_string()),
		]);

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		assert_eq!(merged.len(), 3);
		let key_a_count = merged.iter().filter(|v| v.name == "KEY_A").count();
		assert_eq!(key_a_count, 1);
	}

	#[rstest]
	fn merge_env_vars_empty_user_vars_returns_auto() {
		// Arrange
		let auto_vars = vec![env_var("A", "1"), env_var("B", "2")];
		let user_vars = BTreeMap::new();

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		assert_eq!(merged.len(), 2);
	}

	#[rstest]
	fn merge_env_vars_empty_user_vars_preserves_all_auto() {
		// Arrange
		let auto_vars = vec![
			env_var("REINHARDT_ENV", "production"),
			env_var(
				"REINHARDT_CLOUD_CONFIG_DIR",
				"/etc/reinhardt-cloud/settings",
			),
			env_var("DATABASE_URL", "postgres://localhost/db"),
		];
		let user_vars = BTreeMap::new();

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		assert_eq!(merged.len(), 3);
		assert!(
			merged
				.iter()
				.any(|v| v.name == "REINHARDT_ENV" && v.value.as_deref() == Some("production"))
		);
		assert!(
			merged
				.iter()
				.any(|v| v.name == "REINHARDT_CLOUD_CONFIG_DIR")
		);
		assert!(merged.iter().any(|v| v.name == "DATABASE_URL"));
	}

	#[rstest]
	fn merge_env_vars_user_overrides_all_auto() {
		// Arrange
		let auto_vars = vec![
			env_var("REINHARDT_ENV", "production"),
			env_var(
				"REINHARDT_CLOUD_CONFIG_DIR",
				"/etc/reinhardt-cloud/settings",
			),
		];
		let user_vars = BTreeMap::from([
			("REINHARDT_ENV".to_string(), "staging".to_string()),
			(
				"REINHARDT_CLOUD_CONFIG_DIR".to_string(),
				"/custom/path".to_string(),
			),
		]);

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		assert_eq!(merged.len(), 2);
		let env = merged.iter().find(|v| v.name == "REINHARDT_ENV").unwrap();
		assert_eq!(env.value.as_deref(), Some("staging"));
		let config = merged
			.iter()
			.find(|v| v.name == "REINHARDT_CLOUD_CONFIG_DIR")
			.unwrap();
		assert_eq!(config.value.as_deref(), Some("/custom/path"));
	}

	#[rstest]
	fn build_system_env_vars_always_present() {
		// Arrange & Act
		let vars = build_system_env_vars();

		// Assert
		let names: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
		assert!(names.contains(&"REINHARDT_ENV"));
		assert!(names.contains(&"REINHARDT_CLOUD_CONFIG_DIR"));
	}

	#[rstest]
	fn merge_env_vars_empty_btreemap_merges_correctly() {
		// Arrange
		let auto_vars: Vec<EnvVar> = vec![];
		let user_vars: BTreeMap<String, String> = BTreeMap::new();

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		assert!(merged.is_empty());
	}

	#[rstest]
	fn merge_env_vars_empty_auto_vars_returns_user() {
		// Arrange
		let auto_vars: Vec<EnvVar> = vec![];
		let user_vars = BTreeMap::from([("X".to_string(), "y".to_string())]);

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		assert_eq!(merged.len(), 1);
		assert_eq!(merged[0].name, "X");
	}
}
