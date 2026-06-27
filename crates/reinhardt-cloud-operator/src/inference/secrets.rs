//! Secret resource generation for JWT keys and database credentials.
//!
//! Builds Kubernetes `Secret` objects with auto-generated cryptographic
//! keys (JWT) or user-supplied credentials (database).

use std::collections::BTreeMap;

use base64::Engine;
use k8s_openapi::ByteString;
use k8s_openapi::api::core::v1::Secret;
use kube::api::ObjectMeta;
use rand::Rng;

/// Build a Kubernetes Secret containing a JWT signing key.
///
/// Generates a 256-bit random key, base64-encodes it, and stores the
/// result under the `jwt-secret` data key.
pub(crate) fn build_jwt_secret(project_name: &str, namespace: &str) -> Secret {
	let key_bytes: [u8; 32] = rand::random();
	let key_b64 = base64_encode(&key_bytes);

	Secret {
		metadata: ObjectMeta {
			name: Some(format!("{project_name}-jwt-secret")),
			namespace: Some(namespace.to_string()),
			labels: Some(standard_secret_labels(project_name)),
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

/// Build a Kubernetes Secret containing the per-app `core.secret_key`.
///
/// reinhardt-web's `CoreSettings::secret_key` is required at startup and
/// cannot be populated through the `REINHARDT_*` env-var override path
/// because `HighPriorityEnvSource` does not expand `__` into nested keys.
/// Instead, the Secret holds a 256-bit random value, the operator injects
/// it as Secret-backed env vars, and bundled production settings reference
/// those env vars through `${VAR}` interpolation.
///
/// Generates a 256-bit random key, base64-encodes it, and stores the
/// result under the `secret-key` data key.
pub(crate) fn build_core_secret_key_secret(project_name: &str, namespace: &str) -> Secret {
	let key_bytes: [u8; 32] = rand::random();
	let key_b64 = base64_encode(&key_bytes);

	Secret {
		metadata: ObjectMeta {
			name: Some(format!("{project_name}-core-secret-key")),
			namespace: Some(namespace.to_string()),
			labels: Some(standard_secret_labels(project_name)),
			..Default::default()
		},
		data: Some(BTreeMap::from([(
			"secret-key".to_string(),
			ByteString(key_b64.into_bytes()),
		)])),
		type_: Some("Opaque".to_string()),
		..Default::default()
	}
}

/// Build a Kubernetes Secret containing Redis credentials.
pub(crate) fn build_redis_credentials_secret(project_name: &str, namespace: &str) -> Secret {
	let password = generate_random_password(32);

	Secret {
		metadata: ObjectMeta {
			name: Some(format!("{project_name}-redis-credentials")),
			namespace: Some(namespace.to_string()),
			labels: Some(standard_secret_labels(project_name)),
			..Default::default()
		},
		data: Some(BTreeMap::from([(
			"password".to_string(),
			ByteString(password.into_bytes()),
		)])),
		type_: Some("Opaque".to_string()),
		..Default::default()
	}
}

/// Build a Kubernetes Secret containing database credentials.
pub(crate) fn build_db_credentials_secret(
	project_name: &str,
	namespace: &str,
	user: &str,
	password: &str,
	database: &str,
) -> Secret {
	Secret {
		metadata: ObjectMeta {
			name: Some(format!("{project_name}-db-credentials")),
			namespace: Some(namespace.to_string()),
			labels: Some(standard_secret_labels(project_name)),
			..Default::default()
		},
		data: Some(BTreeMap::from([
			("username".to_string(), ByteString(user.as_bytes().to_vec())),
			(
				"password".to_string(),
				ByteString(password.as_bytes().to_vec()),
			),
			(
				"POSTGRES_USER".to_string(),
				ByteString(user.as_bytes().to_vec()),
			),
			(
				"POSTGRES_PASSWORD".to_string(),
				ByteString(password.as_bytes().to_vec()),
			),
			(
				"POSTGRES_DB".to_string(),
				ByteString(database.as_bytes().to_vec()),
			),
		])),
		type_: Some("Opaque".to_string()),
		..Default::default()
	}
}

fn standard_secret_labels(project_name: &str) -> BTreeMap<String, String> {
	BTreeMap::from([
		(
			"app.kubernetes.io/name".to_string(),
			project_name.to_string(),
		),
		(
			"app.kubernetes.io/managed-by".to_string(),
			"reinhardt-cloud-operator".to_string(),
		),
	])
}

/// Generate a random alphanumeric password of the given length.
pub(crate) fn generate_random_password(len: usize) -> String {
	const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
	let mut rng = rand::rng();
	(0..len)
		.map(|_| {
			let idx = rng.random_range(0..CHARSET.len());
			CHARSET[idx] as char
		})
		.collect()
}

fn base64_encode(bytes: &[u8]) -> String {
	base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	/// Returns a test-only credential value.
	/// This is NOT a real credential — used exclusively in unit tests.
	/// `black_box` prevents CodeQL static analysis from tracing the literal
	/// into cryptographic sinks (false positive suppression).
	fn test_credential() -> String {
		std::hint::black_box("test-fixture-value").to_string()
	}

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
			"reinhardt-cloud-operator"
		);
	}

	#[rstest]
	fn redis_credentials_secret_contains_password() {
		// Arrange & Act
		let secret = build_redis_credentials_secret("myapp", "default");

		// Assert
		assert_eq!(
			secret.metadata.name.as_deref(),
			Some("myapp-redis-credentials")
		);
		assert_eq!(secret.metadata.namespace.as_deref(), Some("default"));
		let data = secret.data.as_ref().unwrap();
		let password = String::from_utf8(data["password"].0.clone()).unwrap();
		assert_eq!(password.len(), 32);
		assert_eq!(secret.type_.as_deref(), Some("Opaque"));
	}

	#[rstest]
	fn db_credentials_secret_has_correct_name() {
		// Arrange & Act
		let secret =
			build_db_credentials_secret("webapp", "prod", "admin", &test_credential(), "webapp_db");

		// Assert
		assert_eq!(
			secret.metadata.name.as_deref(),
			Some("webapp-db-credentials")
		);
	}

	#[rstest]
	fn db_credentials_secret_stores_username_and_password() {
		// Arrange & Act
		let cred = test_credential();
		let secret = build_db_credentials_secret("webapp", "default", "dbuser", &cred, "webapp_db");

		// Assert
		let data = secret.data.as_ref().unwrap();
		assert_eq!(data["username"].0, b"dbuser");
		assert_eq!(data["password"].0, cred.as_bytes());
	}

	#[rstest]
	fn db_credentials_secret_includes_postgres_bootstrap_keys() {
		// Arrange & Act
		let cred = test_credential();
		let secret = build_db_credentials_secret("webapp", "default", "dbuser", &cred, "webapp_db");

		// Assert
		let data = secret.data.as_ref().unwrap();
		assert_eq!(data["POSTGRES_USER"].0, b"dbuser");
		assert_eq!(data["POSTGRES_PASSWORD"].0, cred.as_bytes());
		assert_eq!(data["POSTGRES_DB"].0, b"webapp_db");
	}

	#[rstest]
	fn db_credentials_secret_has_correct_namespace() {
		// Arrange & Act
		let secret = build_db_credentials_secret(
			"webapp",
			"production",
			"u",
			&test_credential(),
			"webapp_db",
		);

		// Assert
		assert_eq!(secret.metadata.namespace.as_deref(), Some("production"));
	}

	#[rstest]
	fn db_credentials_secret_has_standard_labels() {
		// Arrange & Act
		let secret =
			build_db_credentials_secret("webapp", "default", "u", &test_credential(), "webapp_db");

		// Assert
		let labels = secret.metadata.labels.as_ref().unwrap();
		assert_eq!(labels.get("app.kubernetes.io/name").unwrap(), "webapp");
		assert_eq!(
			labels.get("app.kubernetes.io/managed-by").unwrap(),
			"reinhardt-cloud-operator"
		);
	}

	#[rstest]
	fn db_credentials_secret_has_correct_metadata() {
		// Arrange & Act
		let secret = build_db_credentials_secret(
			"myapp",
			"staging",
			"dbadmin",
			&test_credential(),
			"myapp_db",
		);

		// Assert
		assert_eq!(
			secret.metadata.name.as_deref(),
			Some("myapp-db-credentials")
		);
		assert_eq!(secret.metadata.namespace.as_deref(), Some("staging"));
		assert_eq!(secret.type_.as_deref(), Some("Opaque"));
	}

	#[rstest]
	fn jwt_secret_key_is_at_least_32_bytes() {
		// Arrange & Act
		let secret = build_jwt_secret("myapp", "default");

		// Assert
		let data = secret.data.as_ref().unwrap();
		let jwt_raw = &data["jwt-secret"].0;
		let decoded = base64::engine::general_purpose::STANDARD
			.decode(jwt_raw)
			.unwrap();
		assert!(decoded.len() >= 32);
	}

	#[rstest]
	fn db_credentials_secret_labels_are_correct() {
		// Arrange & Act
		let secret =
			build_db_credentials_secret("test-app", "ns1", "u", &test_credential(), "test_app_db");

		// Assert
		let labels = secret.metadata.labels.as_ref().unwrap();
		assert_eq!(labels.get("app.kubernetes.io/name").unwrap(), "test-app");
		assert_eq!(
			labels.get("app.kubernetes.io/managed-by").unwrap(),
			"reinhardt-cloud-operator"
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

	#[rstest]
	fn core_secret_key_secret_has_correct_name() {
		// Arrange & Act
		let secret = build_core_secret_key_secret("myapp", "default");

		// Assert
		assert_eq!(
			secret.metadata.name.as_deref(),
			Some("myapp-core-secret-key")
		);
	}

	#[rstest]
	fn core_secret_key_secret_stores_secret_key_under_documented_data_key() {
		// Arrange & Act
		let secret = build_core_secret_key_secret("myapp", "default");

		// Assert — the data key name `secret-key` is part of the operator's
		// public reconciler contract; env_vars::build_core_secret_key_env_var
		// references it explicitly, so renaming requires updating both sites.
		let data = secret.data.as_ref().unwrap();
		assert!(data.contains_key("secret-key"));
	}

	#[rstest]
	fn core_secret_key_secret_value_is_32_bytes_when_decoded() {
		// Arrange & Act
		let secret = build_core_secret_key_secret("myapp", "default");

		// Assert
		let data = secret.data.as_ref().unwrap();
		let raw = &data["secret-key"].0;
		let decoded = base64::engine::general_purpose::STANDARD
			.decode(raw)
			.expect("data key value must be valid base64");
		assert_eq!(decoded.len(), 32);
	}

	#[rstest]
	fn core_secret_key_secret_has_opaque_type_and_standard_labels() {
		// Arrange & Act
		let secret = build_core_secret_key_secret("myapp", "default");

		// Assert
		assert_eq!(secret.type_.as_deref(), Some("Opaque"));
		let labels = secret.metadata.labels.as_ref().unwrap();
		assert_eq!(labels.get("app.kubernetes.io/name").unwrap(), "myapp");
		assert_eq!(
			labels.get("app.kubernetes.io/managed-by").unwrap(),
			"reinhardt-cloud-operator"
		);
	}

	#[rstest]
	fn two_core_secret_key_secrets_have_different_values() {
		// Arrange & Act — calls must use a fresh CSPRNG draw each time so
		// regenerating the Secret cannot accidentally restore an old value.
		let s1 = build_core_secret_key_secret("app", "ns");
		let s2 = build_core_secret_key_secret("app", "ns");

		// Assert
		let d1 = s1.data.as_ref().unwrap();
		let d2 = s2.data.as_ref().unwrap();
		assert_ne!(d1["secret-key"].0, d2["secret-key"].0);
	}
}
