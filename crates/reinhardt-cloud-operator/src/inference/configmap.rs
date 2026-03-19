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
	// allowed_hosts defaults to empty (deny all) for security.
	// Users should override via REINHARDT_ALLOWED_HOSTS env var
	// or by customizing this ConfigMap after deployment.
	let production_toml = r#"debug = false
allowed_hosts = []

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
	fn configmap_production_toml_has_debug_false() {
		// Arrange & Act
		let cm = build_settings_configmap("myapp", "default");

		// Assert
		let data = cm.data.as_ref().unwrap();
		let toml_content = &data["production.toml"];
		assert!(toml_content.contains("debug = false"));
	}

	#[rstest]
	fn configmap_production_toml_has_security_section() {
		// Arrange & Act
		let cm = build_settings_configmap("myapp", "default");

		// Assert
		let data = cm.data.as_ref().unwrap();
		let toml_content = &data["production.toml"];
		assert!(toml_content.contains("[security]"));
		assert!(toml_content.contains("session_cookie_secure = true"));
		assert!(toml_content.contains("csrf_cookie_secure = true"));
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
