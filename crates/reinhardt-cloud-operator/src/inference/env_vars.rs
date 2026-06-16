//! Environment variable auto-injection and merging for deployed applications.
//!
//! Provides builders for database connection strings, system variables, and
//! a priority-based merge function where user overrides always win.

use std::collections::{BTreeMap, HashSet};

use k8s_openapi::api::core::v1::{EnvVar, EnvVarSource, SecretKeySelector};
use kube::ResourceExt;
use reinhardt_cloud_types::crd::Project;

use super::platform::Platform;

/// Build database connection environment variables that reference a
/// Kubernetes `Secret` for the password, keeping the sensitive value out
/// of the pod spec.
///
/// Non-secret values (host, port, db name, user) are derived from
/// conventions shared with `infer_database_resources`:
/// - Secret name: `{project_name}-db-credentials` (keys `username` / `password`)
/// - Host / port / db name / user: derived from `db_host` and the naming
///   convention used by the inference module. These are deterministic
///   identifiers (not credentials), so embedding them as plain values is safe.
///
/// The caller is responsible for ensuring a PostgreSQL database will be
/// provisioned for this app and that the corresponding credentials Secret
/// (`{project_name}-db-credentials`) will exist.
///
/// The `db_host` parameter must be the DNS name of the Service that exposes
/// the database:
/// - For the explicit `spec.database` path: `"{project_name}-db"` (the headless
///   Service created by `infer_database_resources`).
/// - For the introspect-provisioned path: `"{project_name}-postgresql"` (the
///   Service created by `reconcile_db_service_resource`).
pub(crate) fn build_database_env_vars_from_secret(
	app: &Project,
	platform: &Platform,
	db_host: &str,
	user_vars: &BTreeMap<String, String>,
) -> Vec<EnvVar> {
	let project_name = app.name_any();
	let secret_name = format!("{project_name}-db-credentials");

	let default_port = 5432;
	let host = user_vars
		.get("REINHARDT_DATABASE_HOST")
		.filter(|value| !value.is_empty())
		.cloned()
		.unwrap_or_else(|| db_host.to_string());
	if matches!(platform, Platform::Aws | Platform::Gcp) && host == db_host {
		tracing::warn!(
			app = %project_name,
			"Cloud-managed database endpoint resolution is not yet implemented. \
			 DATABASE_URL and REINHARDT_DATABASE_HOST will use the placeholder host \
			 '{}'. Provide REINHARDT_DATABASE_HOST or DATABASE_URL via spec.env \
			 until cloud endpoint resolution is supported.",
			db_host
		);
	}

	// Both identifiers use replace('-', "_") to produce valid SQL identifiers,
	// matching the convention used by infer_database_resources in inference/database.rs.
	let sanitized_name = project_name.replace('-', "_");
	let db_name = user_vars
		.get("REINHARDT_DATABASE_NAME")
		.filter(|value| !value.is_empty())
		.cloned()
		.unwrap_or_else(|| format!("{sanitized_name}_db"));
	let db_user = user_vars
		.get("REINHARDT_DATABASE_USER")
		.filter(|value| !value.is_empty())
		.cloned()
		.unwrap_or(sanitized_name);
	let port = user_vars
		.get("REINHARDT_DATABASE_PORT")
		.and_then(|value| value.parse::<u16>().ok())
		.unwrap_or(default_port);

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
/// * `project_name` — value for `OTEL_SERVICE_NAME` (typically the app's name).
///
/// Variables injected:
/// * `TRACEPARENT` — W3C `traceparent` of the current reconcile span (omitted
///   when the current span context is not valid). Note: OTel SDKs do not read
///   `TRACEPARENT` automatically; the application must bootstrap context by
///   reading this variable and explicitly setting it as the parent for the
///   process's root span.
/// * `OTEL_PROPAGATORS` — fixed to `tracecontext`.
/// * `OTEL_SERVICE_NAME` — set to `project_name`.
/// * `OTEL_EXPORTER_OTLP_ENDPOINT` — forwarded from the operator's own
///   environment variable of the same name when present.
pub(crate) fn build_otel_env_vars(project_name: &str) -> Vec<EnvVar> {
	use opentelemetry::trace::TraceContextExt;
	use tracing::Span;
	use tracing_opentelemetry::OpenTelemetrySpanExt;

	let mut vars = vec![
		env_var("OTEL_PROPAGATORS", "tracecontext"),
		env_var("OTEL_SERVICE_NAME", project_name),
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
///
/// As of #589, the operator no longer emits an application settings ConfigMap;
/// each reinhardt-web image is responsible for its own bundled
/// `production.toml` (made self-contained in #588), so the previous
/// `REINHARDT_CLOUD_CONFIG_DIR=/etc/reinhardt-cloud/settings` override has
/// been removed. The application's compile-time `CARGO_MANIFEST_DIR/settings`
/// path now wins.
pub(crate) fn build_system_env_vars() -> Vec<EnvVar> {
	vec![env_var("REINHARDT_ENV", "production")]
}

/// Build env vars referencing the per-app `<app>-core-secret-key` Secret
/// created by `inference::secrets::build_core_secret_key_secret`.
///
/// Both names reference the same Secret data key. `REINHARDT_CORE__SECRET_KEY`
/// matches Dashboard production settings, while `REINHARDT_CLOUD_SECRET_KEY`
/// preserves the existing generated-settings contract.
pub(crate) fn build_core_secret_key_env_vars(project_name: &str) -> Vec<EnvVar> {
	vec![
		build_core_secret_key_env_var("REINHARDT_CORE__SECRET_KEY", project_name),
		build_core_secret_key_env_var("REINHARDT_CLOUD_SECRET_KEY", project_name),
	]
}

fn build_core_secret_key_env_var(name: &str, project_name: &str) -> EnvVar {
	EnvVar {
		name: name.to_string(),
		value: None,
		value_from: Some(EnvVarSource {
			secret_key_ref: Some(SecretKeySelector {
				name: format!("{project_name}-core-secret-key"),
				key: "secret-key".to_string(),
				optional: Some(false),
			}),
			..Default::default()
		}),
	}
}

/// Build the `REINHARDT_CLOUD_JWT_SECRET` env var referencing the per-app
/// `<app>-jwt-secret` Secret created when `spec.auth.jwt` is enabled.
pub(crate) fn build_jwt_secret_env_var(project_name: &str) -> EnvVar {
	EnvVar {
		name: "REINHARDT_CLOUD_JWT_SECRET".to_string(),
		value: None,
		value_from: Some(EnvVarSource {
			secret_key_ref: Some(SecretKeySelector {
				name: format!("{project_name}-jwt-secret"),
				key: "jwt-secret".to_string(),
				optional: Some(false),
			}),
			..Default::default()
		}),
	}
}

/// Build the Redis URL env var for the operator-managed Redis Service.
pub(crate) fn build_redis_cache_env_var(project_name: &str) -> EnvVar {
	env_var(
		"REINHARDT_CLOUD_REDIS_URL",
		&format!("redis://{project_name}-redis:6379/0"),
	)
}

/// Merge auto-generated and user-supplied environment variables.
///
/// User overrides (`user_vars`) always take priority over auto-generated
/// variables (`auto_vars`). When both define the same key, the user value
/// is kept and the auto-generated value is discarded.
///
/// User values that match the `secretRef:<secret-name>/<key>` form are
/// rewritten to a Kubernetes `valueFrom.secretKeyRef`, so the rendered
/// `EnvVar` references a `Secret` rather than carrying the literal string
/// (which would otherwise leak the prefix into the running container).
/// Values that do not match the syntax are forwarded as literals.
pub(crate) fn merge_env_vars(
	auto_vars: &[EnvVar],
	user_vars: &BTreeMap<String, String>,
) -> Vec<EnvVar> {
	let mut result: Vec<EnvVar> = Vec::new();
	let mut seen = HashSet::new();

	// User vars first (highest priority)
	for (k, v) in user_vars {
		result.push(user_env_var(k, v));
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

/// Prefix that marks a user-supplied env value as a reference into a
/// Kubernetes `Secret`. The body after the prefix is `<secret-name>/<key>`.
const SECRET_REF_PREFIX: &str = "secretRef:";

/// Parse `secretRef:<secret-name>/<key>`, returning `(secret_name, key)` on
/// success. Both components must be non-empty; a malformed value (missing
/// slash or empty component) yields `None` so the caller can fall back to
/// a literal `value`.
fn parse_secret_ref(value: &str) -> Option<(&str, &str)> {
	let body = value.strip_prefix(SECRET_REF_PREFIX)?;
	let (secret_name, key) = body.split_once('/')?;
	if secret_name.is_empty() || key.is_empty() {
		return None;
	}
	Some((secret_name, key))
}

/// Build an `EnvVar` from a user-supplied `(name, value)` pair, honoring
/// the `secretRef:<secret-name>/<key>` syntax for `Secret` references.
fn user_env_var(name: &str, value: &str) -> EnvVar {
	if let Some((secret_name, key)) = parse_secret_ref(value) {
		return EnvVar {
			name: name.to_string(),
			value: None,
			value_from: Some(EnvVarSource {
				secret_key_ref: Some(SecretKeySelector {
					name: secret_name.to_string(),
					key: key.to_string(),
					optional: Some(false),
				}),
				..Default::default()
			}),
		};
	}

	if value.starts_with(SECRET_REF_PREFIX) {
		// Prefix present but body is malformed. Fall back to a literal
		// to avoid silently dropping the user's value, and warn so the
		// misconfiguration is visible in operator logs.
		tracing::warn!(
			env_name = %name,
			"User-supplied env var {name} starts with `secretRef:` but is not in the form \
			 `secretRef:<secret-name>/<key>`; treating the value as a literal string. \
			 Update the spec to use the documented form to inject a Secret value.",
		);
	}

	env_var(name, value)
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
	use reinhardt_cloud_types::crd::ProjectSpec;
	use reinhardt_cloud_types::crd::database::{DatabaseEngine, DatabaseSpec};
	use rstest::rstest;

	fn make_app_with_db(name: &str) -> Project {
		Project {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid".to_string()),
				..Default::default()
			},
			spec: ProjectSpec {
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
		let vars = build_database_env_vars_from_secret(
			&app,
			&Platform::Onpremise,
			"myapp-db",
			&BTreeMap::new(),
		);

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
		let vars = build_database_env_vars_from_secret(
			&app,
			&Platform::Onpremise,
			"my-app-db",
			&BTreeMap::new(),
		);

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
		let vars = build_database_env_vars_from_secret(
			&app,
			&Platform::Onpremise,
			"my-app-db",
			&BTreeMap::new(),
		);

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
	fn build_database_env_vars_from_secret_uses_settings_derived_overrides() {
		// Arrange
		let app = make_app_with_db("my-app");
		let user_vars = BTreeMap::from([
			(
				"REINHARDT_DATABASE_HOST".to_string(),
				"cloudsql.internal".to_string(),
			),
			("REINHARDT_DATABASE_PORT".to_string(), "6543".to_string()),
			("REINHARDT_DATABASE_NAME".to_string(), "prod_db".to_string()),
			(
				"REINHARDT_DATABASE_USER".to_string(),
				"prod_user".to_string(),
			),
		]);

		// Act
		let vars =
			build_database_env_vars_from_secret(&app, &Platform::Gcp, "my-app-db", &user_vars);

		// Assert
		let host = vars
			.iter()
			.find(|v| v.name == "REINHARDT_DATABASE_HOST")
			.unwrap();
		assert_eq!(host.value.as_deref(), Some("cloudsql.internal"));

		let url = vars
			.iter()
			.find(|v| v.name == "DATABASE_URL")
			.and_then(|v| v.value.as_deref())
			.unwrap();
		assert_eq!(
			url,
			"postgres://prod_user:$(REINHARDT_DATABASE_PASSWORD)@cloudsql.internal:6543/prod_db"
		);
	}

	#[rstest]
	fn build_system_env_vars_contains_required_keys() {
		// Arrange & Act
		let vars = build_system_env_vars();

		// Assert — after #589, `REINHARDT_ENV` is the only system env var
		// the operator injects. `REINHARDT_CLOUD_CONFIG_DIR` was dropped
		// because the application reads its own bundled production.toml
		// (made self-contained via ${VAR} interpolation in #588).
		assert_eq!(vars.len(), 1);

		let env_var = vars.iter().find(|v| v.name == "REINHARDT_ENV").unwrap();
		assert_eq!(env_var.value.as_deref(), Some("production"));

		assert!(
			!vars.iter().any(|v| v.name == "REINHARDT_CLOUD_CONFIG_DIR"),
			"REINHARDT_CLOUD_CONFIG_DIR must not be auto-injected after #589",
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
			env_var("DATABASE_URL", "postgres://localhost/db"),
		];
		let user_vars = BTreeMap::new();

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		assert_eq!(merged.len(), 2);
		assert!(
			merged
				.iter()
				.any(|v| v.name == "REINHARDT_ENV" && v.value.as_deref() == Some("production"))
		);
		assert!(merged.iter().any(|v| v.name == "DATABASE_URL"));
	}

	#[rstest]
	fn merge_env_vars_user_overrides_all_auto() {
		// Arrange
		let auto_vars = vec![env_var("REINHARDT_ENV", "production")];
		let user_vars = BTreeMap::from([("REINHARDT_ENV".to_string(), "staging".to_string())]);

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert — user override takes precedence over the auto-injected
		// `REINHARDT_ENV`, and no other system env var leaks through.
		assert_eq!(merged.len(), 1);
		let env = merged.iter().find(|v| v.name == "REINHARDT_ENV").unwrap();
		assert_eq!(env.value.as_deref(), Some("staging"));
	}

	#[rstest]
	fn build_system_env_vars_always_present() {
		// Arrange & Act
		let vars = build_system_env_vars();

		// Assert — `REINHARDT_ENV` remains the single system env var the
		// operator injects after #589. `REINHARDT_CLOUD_CONFIG_DIR` was
		// removed alongside the application settings ConfigMap.
		let names: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
		assert_eq!(names, vec!["REINHARDT_ENV"]);
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

	#[rstest]
	fn core_secret_key_env_vars_reference_per_app_secret() {
		// Arrange & Act
		let vars = build_core_secret_key_env_vars("myapp");

		// Assert — the actual key bytes must come from the Secret, never
		// be inlined into the Pod spec. Dashboard production settings read
		// `REINHARDT_CORE__SECRET_KEY`; the legacy cloud name remains for
		// generated-settings compatibility.
		let names: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
		assert_eq!(
			names,
			vec!["REINHARDT_CORE__SECRET_KEY", "REINHARDT_CLOUD_SECRET_KEY"]
		);
		for var in vars {
			assert!(var.value.is_none());
			let key_ref = var
				.value_from
				.as_ref()
				.and_then(|vf| vf.secret_key_ref.as_ref())
				.expect("must use SecretKeyRef so the key value is never inlined");
			assert_eq!(key_ref.name, "myapp-core-secret-key");
			assert_eq!(key_ref.key, "secret-key");
			assert_eq!(key_ref.optional, Some(false));
		}
	}

	#[rstest]
	fn core_secret_key_env_vars_secret_name_tracks_project_name() {
		// Arrange & Act — a different app must reference its own Secret;
		// the helper must NOT share a single Secret across apps.
		let vars = build_core_secret_key_env_vars("other-app");

		// Assert
		for var in vars {
			let key_ref = var
				.value_from
				.as_ref()
				.and_then(|vf| vf.secret_key_ref.as_ref())
				.unwrap();
			assert_eq!(key_ref.name, "other-app-core-secret-key");
		}
	}

	#[rstest]
	fn jwt_secret_env_var_references_per_app_secret() {
		// Arrange & Act
		let var = build_jwt_secret_env_var("dashboard");

		// Assert
		assert_eq!(var.name, "REINHARDT_CLOUD_JWT_SECRET");
		assert!(var.value.is_none());
		let key_ref = var
			.value_from
			.as_ref()
			.and_then(|vf| vf.secret_key_ref.as_ref())
			.expect("JWT secret must be Secret-backed");
		assert_eq!(key_ref.name, "dashboard-jwt-secret");
		assert_eq!(key_ref.key, "jwt-secret");
		assert_eq!(key_ref.optional, Some(false));
	}

	#[rstest]
	fn redis_cache_env_var_uses_operator_managed_service() {
		// Arrange & Act
		let var = build_redis_cache_env_var("dashboard");

		// Assert
		assert_eq!(var.name, "REINHARDT_CLOUD_REDIS_URL");
		assert_eq!(var.value.as_deref(), Some("redis://dashboard-redis:6379/0"));
		assert!(var.value_from.is_none());
	}

	#[rstest]
	fn merge_env_vars_resolves_secret_ref_to_value_from() {
		// Arrange — `manifests/dashboard-app.yaml` uses this exact form.
		let auto_vars: Vec<EnvVar> = vec![];
		let user_vars = BTreeMap::from([(
			"REINHARDT_CLOUD_JWT_SECRET".to_string(),
			"secretRef:reinhardt-cloud-dashboard-secrets/jwt-secret".to_string(),
		)]);

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert — literal value must be cleared and a SecretKeyRef emitted.
		let var = merged
			.iter()
			.find(|v| v.name == "REINHARDT_CLOUD_JWT_SECRET")
			.expect("env var must be present");
		assert!(
			var.value.is_none(),
			"value must be empty when a SecretKeyRef is set; the operator must not pass the \
			 `secretRef:...` literal through to the container env",
		);
		let key_ref = var
			.value_from
			.as_ref()
			.and_then(|vf| vf.secret_key_ref.as_ref())
			.expect("value_from.secret_key_ref must be set");
		assert_eq!(key_ref.name, "reinhardt-cloud-dashboard-secrets");
		assert_eq!(key_ref.key, "jwt-secret");
		assert_eq!(key_ref.optional, Some(false));
	}

	#[rstest]
	#[case::missing_slash("secretRef:onlyname")]
	#[case::empty_secret_name("secretRef:/key")]
	#[case::empty_key("secretRef:name/")]
	#[case::empty_body("secretRef:")]
	fn merge_env_vars_falls_back_to_literal_for_malformed_secret_ref(#[case] value: &str) {
		// Arrange
		let auto_vars: Vec<EnvVar> = vec![];
		let user_vars = BTreeMap::from([("MY_VAR".to_string(), value.to_string())]);

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert — malformed prefix is preserved as a literal so we don't
		// silently drop the user's value; a tracing warn surfaces the issue.
		let var = merged.iter().find(|v| v.name == "MY_VAR").unwrap();
		assert_eq!(var.value.as_deref(), Some(value));
		assert!(var.value_from.is_none());
	}

	#[rstest]
	fn merge_env_vars_does_not_match_prefix_inside_value() {
		// Arrange — only the literal prefix at position 0 should be parsed,
		// otherwise URLs containing `secretRef:` mid-string would be eaten.
		let auto_vars: Vec<EnvVar> = vec![];
		let user_vars = BTreeMap::from([(
			"NOTE".to_string(),
			"see secretRef:foo/bar in the docs".to_string(),
		)]);

		// Act
		let merged = merge_env_vars(&auto_vars, &user_vars);

		// Assert
		let var = merged.iter().find(|v| v.name == "NOTE").unwrap();
		assert_eq!(
			var.value.as_deref(),
			Some("see secretRef:foo/bar in the docs")
		);
		assert!(var.value_from.is_none());
	}
}
