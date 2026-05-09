//! Settings ConfigMap generation.
//!
//! Builds a Kubernetes `ConfigMap` containing production settings
//! that are mounted into the application container.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::ConfigMap;
use kube::api::ObjectMeta;

/// Build a ConfigMap containing production settings for the app.
///
/// The ConfigMap contains a `production.toml` key with sensible
/// production defaults (debug off, secure cookies, etc.).
pub(crate) fn build_settings_configmap(app_name: &str, namespace: &str) -> ConfigMap {
	// The dashboard's `ProjectSettings` (see `dashboard/src/config/settings.rs`)
	// composes `CoreSettings`, `I18nSettings`, `StaticSettings`, `MediaSettings`,
	// `CorsSettings`, and `EmailSettings`; deserialization fails when any of
	// the corresponding sections is absent from the merged TOML. Because the
	// operator overrides `REINHARDT_CLOUD_CONFIG_DIR` to point at this mounted
	// ConfigMap, it must own a complete schema (the bundled
	// `dashboard/settings/production.toml` is shadowed and is itself
	// incomplete — it relies on a developer-managed `base.toml` that is
	// gitignored).
	//
	// Per-app values (DB host/port/name/user/password and the application
	// signing key) are emitted as `${VAR}` placeholders, NOT inline literals.
	// reinhardt-conf resolves them at pod startup against the env vars the
	// operator already injects from `inference::env_vars`:
	//
	//   - `REINHARDT_CLOUD_SECRET_KEY` → secretKeyRef on `<app>-core-secret-key`
	//   - `REINHARDT_DATABASE_HOST/PORT/NAME/USER` → literal values
	//   - `REINHARDT_DATABASE_PASSWORD` → secretKeyRef on `<app>-db-credentials`
	//
	// kent8192/reinhardt-web#4232 (`feat(conf)!: typed TOML interpolation`)
	// makes this work for typed targets too — `port = "${REINHARDT_DATABASE_PORT:-5432}"`
	// deserializes into `u16`, not `String`. Without that change we would have
	// to fall back to operator-side string→int substitution before mounting.
	//
	// Required secrets use the `${VAR:?message}` form so a missing env var
	// surfaces a precise startup error instead of a downstream connect/auth
	// failure that would be harder to attribute back to the operator.
	//
	// `allowed_hosts` defaults to empty (deny all) for security. Users should
	// customize this ConfigMap after deployment when they have a public hostname
	// to allow.
	//
	// `secure_ssl_redirect` is left `false` because the operator's default
	// reinhardt-cloud deployment terminates TLS at the Ingress (or runs without
	// TLS in local kind clusters); enabling redirect inside the pod would loop
	// when the inbound request is already plain HTTP.
	let production_toml = r#"[core]
debug = false
allowed_hosts = []
secret_key = "${REINHARDT_CLOUD_SECRET_KEY:?Operator must inject REINHARDT_CLOUD_SECRET_KEY via secretKeyRef}"
root_urlconf = ""
middleware = []

[core.security]
append_slash = true
session_cookie_secure = true
csrf_cookie_secure = true
secure_ssl_redirect = false
secure_hsts_include_subdomains = false
secure_hsts_preload = false

[core.databases.default]
engine = "postgresql"
host = "${REINHARDT_DATABASE_HOST:-localhost}"
port = "${REINHARDT_DATABASE_PORT:-5432}"
name = "${REINHARDT_DATABASE_NAME:-reinhardt_cloud}"
user = "${REINHARDT_DATABASE_USER:-reinhardt}"
password = { secret = "${REINHARDT_DATABASE_PASSWORD:?Operator must inject DB password via secretKeyRef}" }
options = {}

[i18n]
language_code = "en-us"
time_zone = "UTC"
use_i18n = true
use_tz = true

[static_files]
url = "/static/"
root = "static"

[media]
url = "/media/"
root = "media"

[cors]
allow_origins = []

[email]
host = "localhost"
port = 1025
from_email = "noreply@example.invalid"
use_ssl = false
use_tls = false
"#;

	ConfigMap {
		metadata: ObjectMeta {
			name: Some(format!("{app_name}-settings")),
			namespace: Some(namespace.to_string()),
			labels: Some(BTreeMap::from([
				("app.kubernetes.io/name".to_string(), app_name.to_string()),
				(
					"app.kubernetes.io/managed-by".to_string(),
					"reinhardt-cloud-operator".to_string(),
				),
			])),
			..Default::default()
		},
		data: Some(BTreeMap::from([(
			"production.toml".to_string(),
			production_toml.to_string(),
		)])),
		..Default::default()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	/// Extract and parse the `production.toml` value out of a generated
	/// `ConfigMap`. Centralizes the boilerplate so individual tests can focus
	/// on the section/key they actually assert against.
	fn parse_production_toml(cm: &ConfigMap) -> toml::Value {
		let data = cm.data.as_ref().expect("ConfigMap.data must be set");
		let content = data
			.get("production.toml")
			.expect("`production.toml` key must be present");
		toml::from_str(content).expect("`production.toml` must parse as valid TOML")
	}

	#[rstest]
	fn configmap_has_correct_name() {
		// Arrange & Act
		let cm = build_settings_configmap("myapp", "default");

		// Assert
		assert_eq!(cm.metadata.name.as_deref(), Some("myapp-settings"));
	}

	#[rstest]
	fn configmap_has_correct_namespace() {
		// Arrange & Act
		let cm = build_settings_configmap("myapp", "staging");

		// Assert
		assert_eq!(cm.metadata.namespace.as_deref(), Some("staging"));
	}

	#[rstest]
	fn configmap_contains_production_toml_key() {
		// Arrange & Act
		let cm = build_settings_configmap("myapp", "default");

		// Assert
		let data = cm.data.as_ref().unwrap();
		assert!(data.contains_key("production.toml"));
	}

	#[rstest]
	fn configmap_production_toml_places_debug_under_core_section() {
		// Arrange & Act
		let cm = build_settings_configmap("myapp", "default");

		// Assert — parse the TOML so the section nesting is enforced, not just
		// the textual presence of `debug = false` (which would also pass for
		// the malformed top-level placement that motivated this regression test).
		let parsed = parse_production_toml(&cm);
		assert_eq!(
			parsed
				.get("core")
				.and_then(|c| c.get("debug"))
				.and_then(|v| v.as_bool()),
			Some(false),
			"`debug` must be nested under `[core]`, not at top level",
		);
		assert!(
			parsed.get("debug").is_none(),
			"`debug` must not appear at the top level (reinhardt-conf would discard it)",
		);
	}

	#[rstest]
	fn configmap_production_toml_places_allowed_hosts_under_core_section() {
		// Arrange & Act
		let cm = build_settings_configmap("myapp", "default");

		// Assert
		let parsed = parse_production_toml(&cm);
		let allowed_hosts = parsed
			.get("core")
			.and_then(|c| c.get("allowed_hosts"))
			.and_then(|v| v.as_array())
			.expect("`allowed_hosts` must be a `[core]` array");
		assert!(
			allowed_hosts.is_empty(),
			"default allowed_hosts is empty (deny all) for security",
		);
		assert!(
			parsed.get("allowed_hosts").is_none(),
			"`allowed_hosts` must not appear at the top level",
		);
	}

	#[rstest]
	fn configmap_production_toml_has_core_security_subsection() {
		// Arrange & Act — `CoreSettings::security` is nested, so the section
		// header must be `[core.security]`, not the top-level `[security]`
		// that earlier revisions emitted.
		let cm = build_settings_configmap("myapp", "default");

		// Assert
		let parsed = parse_production_toml(&cm);
		let security = parsed
			.get("core")
			.and_then(|c| c.get("security"))
			.expect("`[core.security]` section must be present");
		assert_eq!(
			security
				.get("session_cookie_secure")
				.and_then(|v| v.as_bool()),
			Some(true),
		);
		assert_eq!(
			security.get("csrf_cookie_secure").and_then(|v| v.as_bool()),
			Some(true),
		);
		assert_eq!(
			security
				.get("secure_ssl_redirect")
				.and_then(|v| v.as_bool()),
			Some(false),
		);
		// And the malformed top-level form must NOT be present.
		assert!(
			parsed.get("security").is_none(),
			"`[security]` must not be a top-level section (reinhardt-conf expects `[core.security]`)",
		);
	}

	#[rstest]
	fn configmap_production_toml_has_all_required_project_settings_sections() {
		// Arrange & Act — `dashboard/src/config/settings.rs` composes
		// ProjectSettings from CoreSettings, I18nSettings, StaticSettings,
		// MediaSettings, CorsSettings, and EmailSettings. The dashboard panics
		// at startup if any of these are missing from the merged TOML, so
		// guard the operator against silently re-introducing the regression.
		let cm = build_settings_configmap("myapp", "default");

		// Assert
		let parsed = parse_production_toml(&cm);
		for section in ["core", "i18n", "static_files", "media", "cors", "email"] {
			assert!(
				parsed.get(section).is_some(),
				"`[{section}]` must be present in production.toml — missing it crashes \
				 reinhardt-web's settings deserializer",
			);
		}
	}

	#[rstest]
	fn configmap_production_toml_databases_default_uses_postgres_with_options_table() {
		// Arrange & Act — `[core.databases.default]` schema requires
		// `engine`, `host`, `port`, `name`, `user`, `password`, and `options`.
		// `engine` and `options` stay literals (operator policy); `host`,
		// `port`, `name`, `user`, and `password.secret` flow through env-var
		// interpolation (asserted by sibling tests).
		let cm = build_settings_configmap("myapp", "default");

		// Assert
		let parsed = parse_production_toml(&cm);
		let default_db = parsed
			.get("core")
			.and_then(|c| c.get("databases"))
			.and_then(|d| d.get("default"))
			.expect("`[core.databases.default]` must be present");
		assert_eq!(
			default_db.get("engine").and_then(|v| v.as_str()),
			Some("postgresql")
		);
		assert!(
			default_db
				.get("password")
				.and_then(|v| v.as_table())
				.is_some(),
			"password must be the `{{ secret = \"...\" }}` shape so reinhardt-conf accepts it"
		);
		assert!(
			default_db
				.get("options")
				.and_then(|v| v.as_table())
				.is_some(),
			"options must be an inline table even when empty"
		);
	}

	#[rstest]
	fn configmap_production_toml_databases_use_env_interpolation_with_defaults() {
		// Arrange & Act — `host`, `port`, `name`, and `user` are emitted as
		// `${REINHARDT_DATABASE_*:-default}` strings so the operator's
		// existing env-var injection (`inference::env_vars`) drives per-app
		// variation through one source. The `port` field doubles as a
		// regression test for kent8192/reinhardt-web#4232: the string
		// `"5432"` must round-trip into `u16` via typed coercion at the
		// dashboard side (covered end-to-end in the integration test crate).
		let cm = build_settings_configmap("myapp", "default");

		// Assert
		let parsed = parse_production_toml(&cm);
		let default_db = parsed
			.get("core")
			.and_then(|c| c.get("databases"))
			.and_then(|d| d.get("default"))
			.expect("`[core.databases.default]` must be present");
		for (field, expected) in [
			("host", "${REINHARDT_DATABASE_HOST:-localhost}"),
			("port", "${REINHARDT_DATABASE_PORT:-5432}"),
			("name", "${REINHARDT_DATABASE_NAME:-reinhardt_cloud}"),
			("user", "${REINHARDT_DATABASE_USER:-reinhardt}"),
		] {
			assert_eq!(
				default_db.get(field).and_then(|v| v.as_str()),
				Some(expected),
				"`core.databases.default.{field}` must reference the operator-injected \
				 env var; inline literals defeat per-app variation",
			);
		}
	}

	#[rstest]
	fn configmap_production_toml_database_password_requires_injected_env_var() {
		// Arrange & Act — `${VAR:?message}` makes a missing env var fail-fast
		// at pod startup with a precise message instead of degrading to a
		// downstream connect/auth error.
		let cm = build_settings_configmap("myapp", "default");

		// Assert
		let parsed = parse_production_toml(&cm);
		let secret = parsed
			.get("core")
			.and_then(|c| c.get("databases"))
			.and_then(|d| d.get("default"))
			.and_then(|d| d.get("password"))
			.and_then(|p| p.get("secret"))
			.and_then(|s| s.as_str())
			.expect("`core.databases.default.password.secret` must be a string");
		assert!(
			secret.starts_with("${REINHARDT_DATABASE_PASSWORD:?"),
			"DB password must use the `${{VAR:?message}}` form so reinhardt-conf \
			 surfaces a startup error when the operator forgot to inject it; got: {secret}",
		);
	}

	#[rstest]
	fn configmap_production_toml_i18n_section_uses_utc_defaults() {
		// Arrange & Act
		let cm = build_settings_configmap("myapp", "default");

		// Assert
		let parsed = parse_production_toml(&cm);
		let i18n = parsed.get("i18n").expect("`[i18n]` must be present");
		assert_eq!(
			i18n.get("language_code").and_then(|v| v.as_str()),
			Some("en-us")
		);
		assert_eq!(i18n.get("time_zone").and_then(|v| v.as_str()), Some("UTC"));
		assert_eq!(i18n.get("use_i18n").and_then(|v| v.as_bool()), Some(true));
		assert_eq!(i18n.get("use_tz").and_then(|v| v.as_bool()), Some(true));
	}

	#[rstest]
	fn configmap_production_toml_cors_default_is_empty_for_safety() {
		// Arrange & Act — production default must NOT permit any cross-origin
		// requests; operators opt in by editing the ConfigMap once they know
		// their public hostname.
		let cm = build_settings_configmap("myapp", "default");

		// Assert
		let parsed = parse_production_toml(&cm);
		let allow_origins = parsed
			.get("cors")
			.and_then(|c| c.get("allow_origins"))
			.and_then(|v| v.as_array())
			.expect("`cors.allow_origins` must be an array");
		assert!(
			allow_origins.is_empty(),
			"production default must deny all cross-origin requests — \
			 a non-empty default would silently widen the attack surface",
		);
	}

	#[rstest]
	fn configmap_production_toml_secret_key_uses_required_env_interpolation() {
		// Arrange & Act
		let cm = build_settings_configmap("myapp", "default");

		// Assert — the placeholder must be the `${VAR:?message}` form so a
		// missing operator-side secret injection surfaces a precise startup
		// error rather than a generic empty-key crash.
		let parsed = parse_production_toml(&cm);
		let secret_key = parsed
			.get("core")
			.and_then(|c| c.get("secret_key"))
			.and_then(|v| v.as_str())
			.expect("`core.secret_key` must be set so reinhardt-conf can interpolate it");
		assert!(
			secret_key.starts_with("${REINHARDT_CLOUD_SECRET_KEY:?"),
			"`core.secret_key` must use the required `${{VAR:?message}}` form; got: {secret_key}",
		);
	}

	#[rstest]
	fn configmap_has_standard_labels() {
		// Arrange & Act
		let cm = build_settings_configmap("myapp", "default");

		// Assert
		let labels = cm.metadata.labels.as_ref().unwrap();
		assert_eq!(labels.get("app.kubernetes.io/name").unwrap(), "myapp");
		assert_eq!(
			labels.get("app.kubernetes.io/managed-by").unwrap(),
			"reinhardt-cloud-operator"
		);
	}
}
