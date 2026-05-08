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
	// `debug` and `allowed_hosts` belong to CoreSettings and must live under
	// `[core]`; placing them at the top level causes reinhardt-conf to warn
	// and silently discard the values at load time.
	//
	// `allowed_hosts` defaults to empty (deny all) for security. Users should
	// override via `REINHARDT_CORE__ALLOWED_HOSTS` env var or by customizing
	// this ConfigMap after deployment.
	//
	// `secret_key` is filled in by reinhardt-conf's `${VAR}` interpolation
	// from `REINHARDT_CLOUD_SECRET_KEY`, which the operator injects into the
	// Pod via `valueFrom.secretKeyRef` against the per-app
	// `<app>-core-secret-key` Secret (see
	// `inference::env_vars::build_core_secret_key_env_var`). Storing only
	// the placeholder here keeps the actual key value out of the ConfigMap.
	let production_toml = r#"[core]
debug = false
allowed_hosts = []
secret_key = "${REINHARDT_CLOUD_SECRET_KEY}"

[security]
session_cookie_secure = true
csrf_cookie_secure = true
secure_ssl_redirect = false
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
		let data = cm.data.as_ref().unwrap();
		let toml_content = &data["production.toml"];
		let parsed: toml::Value = toml::from_str(toml_content).expect("valid TOML");
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
		let data = cm.data.as_ref().unwrap();
		let toml_content = &data["production.toml"];
		let parsed: toml::Value = toml::from_str(toml_content).expect("valid TOML");
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
	fn configmap_production_toml_has_security_section() {
		// Arrange & Act
		let cm = build_settings_configmap("myapp", "default");

		// Assert
		let data = cm.data.as_ref().unwrap();
		let toml_content = &data["production.toml"];
		let parsed: toml::Value = toml::from_str(toml_content).expect("valid TOML");
		let security = parsed
			.get("security")
			.expect("`[security]` section must be present");
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
	}

	#[rstest]
	fn configmap_production_toml_references_secret_key_via_env_var_interpolation() {
		// Arrange & Act
		let cm = build_settings_configmap("myapp", "default");

		// Assert — the placeholder must be present and must NOT carry an
		// inline key value (which would defeat the point of moving the
		// signing key into a Secret).
		let data = cm.data.as_ref().unwrap();
		let toml_content = &data["production.toml"];
		let parsed: toml::Value = toml::from_str(toml_content).expect("valid TOML");
		let secret_key = parsed
			.get("core")
			.and_then(|c| c.get("secret_key"))
			.and_then(|v| v.as_str())
			.expect("`core.secret_key` must be set so reinhardt-conf can interpolate it");
		assert_eq!(
			secret_key, "${REINHARDT_CLOUD_SECRET_KEY}",
			"`core.secret_key` must reference the operator-injected env var, never an inline value",
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
