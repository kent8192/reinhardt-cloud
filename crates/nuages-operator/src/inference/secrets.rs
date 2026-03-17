//! Secret resource generation for JWT keys and database credentials.
//!
//! Builds Kubernetes `Secret` objects with auto-generated cryptographic
//! keys (JWT) or user-supplied credentials (database).

use std::collections::BTreeMap;

use base64::Engine;
use k8s_openapi::ByteString;
use k8s_openapi::api::core::v1::Secret;
use kube::api::ObjectMeta;

/// Build a Kubernetes Secret containing a JWT signing key.
///
/// Generates a 256-bit random key, base64-encodes it, and stores the
/// result under the `jwt-secret` data key.
pub(crate) fn build_jwt_secret(app_name: &str, namespace: &str) -> Secret {
	let key_bytes: [u8; 32] = rand::random();
	let key_b64 = base64_encode(&key_bytes);

	Secret {
		metadata: ObjectMeta {
			name: Some(format!("{app_name}-jwt-secret")),
			namespace: Some(namespace.to_string()),
			labels: Some(standard_secret_labels(app_name)),
			..Default::default()
		},
		data: Some(BTreeMap::from([(
			"jwt-secret".to_string(),
			ByteString(key_b64.into_bytes()),
		)])),
		type_: Some("Opaque".to_string()),
		..Default::default()
	}
}

/// Build a Kubernetes Secret containing database credentials.
pub(crate) fn build_db_credentials_secret(
	app_name: &str,
	namespace: &str,
	user: &str,
	password: &str,
) -> Secret {
	Secret {
		metadata: ObjectMeta {
			name: Some(format!("{app_name}-db-credentials")),
			namespace: Some(namespace.to_string()),
			labels: Some(standard_secret_labels(app_name)),
			..Default::default()
		},
		data: Some(BTreeMap::from([
			("username".to_string(), ByteString(user.as_bytes().to_vec())),
			(
				"password".to_string(),
				ByteString(password.as_bytes().to_vec()),
			),
		])),
		type_: Some("Opaque".to_string()),
		..Default::default()
	}
}

fn standard_secret_labels(app_name: &str) -> BTreeMap<String, String> {
	BTreeMap::from([
		("app.kubernetes.io/name".to_string(), app_name.to_string()),
		(
			"app.kubernetes.io/managed-by".to_string(),
			"nuages-operator".to_string(),
		),
	])
}

fn base64_encode(bytes: &[u8]) -> String {
	base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn jwt_secret_has_correct_name() {
		// Arrange & Act
		let secret = build_jwt_secret("myapp", "default");

		// Assert
		assert_eq!(secret.metadata.name.as_deref(), Some("myapp-jwt-secret"));
	}

	#[rstest]
	fn jwt_secret_has_correct_namespace() {
		// Arrange & Act
		let secret = build_jwt_secret("myapp", "staging");

		// Assert
		assert_eq!(secret.metadata.namespace.as_deref(), Some("staging"));
	}

	#[rstest]
	fn jwt_secret_contains_jwt_secret_key() {
		// Arrange & Act
		let secret = build_jwt_secret("myapp", "default");

		// Assert
		let data = secret.data.as_ref().unwrap();
		assert!(data.contains_key("jwt-secret"));
	}

	#[rstest]
	fn jwt_secret_key_has_valid_base64_length() {
		// Arrange & Act
		let secret = build_jwt_secret("myapp", "default");

		// Assert
		let data = secret.data.as_ref().unwrap();
		let jwt_bytes = &data["jwt-secret"].0;
		// 32 bytes base64-encoded = 44 characters (with padding)
		let decoded = base64::engine::general_purpose::STANDARD
			.decode(jwt_bytes)
			.unwrap();
		assert_eq!(decoded.len(), 32);
	}

	#[rstest]
	fn jwt_secret_has_opaque_type() {
		// Arrange & Act
		let secret = build_jwt_secret("myapp", "default");

		// Assert
		assert_eq!(secret.type_.as_deref(), Some("Opaque"));
	}

	#[rstest]
	fn jwt_secret_has_standard_labels() {
		// Arrange & Act
		let secret = build_jwt_secret("myapp", "default");

		// Assert
		let labels = secret.metadata.labels.as_ref().unwrap();
		assert_eq!(labels.get("app.kubernetes.io/name").unwrap(), "myapp");
		assert_eq!(
			labels.get("app.kubernetes.io/managed-by").unwrap(),
			"nuages-operator"
		);
	}

	#[rstest]
	fn db_credentials_secret_has_correct_name() {
		// Arrange & Act
		let secret = build_db_credentials_secret("webapp", "prod", "admin", "pw123");

		// Assert
		assert_eq!(
			secret.metadata.name.as_deref(),
			Some("webapp-db-credentials")
		);
	}

	#[rstest]
	fn db_credentials_secret_stores_username_and_password() {
		// Arrange & Act
		let secret = build_db_credentials_secret("webapp", "default", "dbuser", "dbpass");

		// Assert
		let data = secret.data.as_ref().unwrap();
		assert_eq!(data["username"].0, b"dbuser");
		assert_eq!(data["password"].0, b"dbpass");
	}

	#[rstest]
	fn db_credentials_secret_has_correct_namespace() {
		// Arrange & Act
		let secret = build_db_credentials_secret("webapp", "production", "u", "p");

		// Assert
		assert_eq!(secret.metadata.namespace.as_deref(), Some("production"));
	}

	#[rstest]
	fn db_credentials_secret_has_standard_labels() {
		// Arrange & Act
		let secret = build_db_credentials_secret("webapp", "default", "u", "p");

		// Assert
		let labels = secret.metadata.labels.as_ref().unwrap();
		assert_eq!(labels.get("app.kubernetes.io/name").unwrap(), "webapp");
		assert_eq!(
			labels.get("app.kubernetes.io/managed-by").unwrap(),
			"nuages-operator"
		);
	}

	#[rstest]
	fn two_jwt_secrets_have_different_keys() {
		// Arrange & Act
		let secret1 = build_jwt_secret("app", "ns");
		let secret2 = build_jwt_secret("app", "ns");

		// Assert
		let data1 = secret1.data.as_ref().unwrap();
		let data2 = secret2.data.as_ref().unwrap();
		assert_ne!(data1["jwt-secret"].0, data2["jwt-secret"].0);
	}
}
