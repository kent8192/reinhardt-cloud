//! SMTP credentials Secret builder for mail-enabled applications.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::Secret;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;
use kube::ResourceExt;
use reinhardt_cloud_types::crd::ReinhardtApp;

use super::labels::{Component, owner_reference, standard_labels};
use crate::error::Error;

/// Builds an opaque `Secret` containing SMTP credential placeholders for the given `ReinhardtApp`.
///
/// The secret includes `SMTP_HOST`, `SMTP_PORT`, `SMTP_USER`, `SMTP_PASSWORD`,
/// and `SMTP_USE_TLS` keys with sensible defaults.
pub(crate) fn build_mail_secret(app: &ReinhardtApp) -> Result<Secret, Error> {
	let labels = standard_labels(app, Component::Web);
	let namespace = app.namespace().unwrap_or_default();
	let owner_ref = owner_reference(app)?;
	let app_name = app.name_any();

	Ok(Secret {
		metadata: ObjectMeta {
			name: Some(format!("{}-smtp-credentials", app_name)),
			namespace: Some(namespace),
			labels: Some(labels),
			owner_references: Some(vec![owner_ref]),
			..Default::default()
		},
		type_: Some("Opaque".to_string()),
		string_data: Some(BTreeMap::from([
			("SMTP_HOST".to_string(), String::new()),
			("SMTP_PORT".to_string(), "587".to_string()),
			("SMTP_USER".to_string(), String::new()),
			("SMTP_PASSWORD".to_string(), String::new()),
			("SMTP_USE_TLS".to_string(), "true".to_string()),
		])),
		..Default::default()
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	fn test_app(name: &str) -> ReinhardtApp {
		let json = serde_json::json!({
			"apiVersion": "paas.reinhardt-cloud.dev/v1alpha2",
			"kind": "ReinhardtApp",
			"metadata": { "name": name, "namespace": "default", "uid": "test-uid" },
			"spec": { "image": "myapp:latest" }
		});
		serde_json::from_value(json).unwrap()
	}

	#[rstest]
	fn test_build_mail_secret_name() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let secret = build_mail_secret(&app).expect("build should succeed");

		// Assert
		assert_eq!(
			secret.metadata.name.as_deref(),
			Some("myapp-smtp-credentials")
		);
	}

	#[rstest]
	fn test_build_mail_secret_keys() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let secret = build_mail_secret(&app).expect("build should succeed");
		let data = secret.string_data.as_ref().unwrap();

		// Assert
		assert!(data.contains_key("SMTP_HOST"));
		assert!(data.contains_key("SMTP_PORT"));
		assert!(data.contains_key("SMTP_USER"));
		assert!(data.contains_key("SMTP_PASSWORD"));
		assert!(data.contains_key("SMTP_USE_TLS"));
		assert_eq!(data.len(), 5);
	}

	#[rstest]
	fn test_build_mail_secret_default_port() {
		// Arrange
		let app = test_app("myapp");

		// Act
		let secret = build_mail_secret(&app).expect("build should succeed");
		let data = secret.string_data.as_ref().unwrap();

		// Assert
		assert_eq!(data.get("SMTP_PORT").unwrap(), "587");
		assert_eq!(data.get("SMTP_USE_TLS").unwrap(), "true");
	}
}
