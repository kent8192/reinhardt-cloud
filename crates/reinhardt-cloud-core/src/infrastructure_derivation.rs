//! Derivation from introspection signals to managed infrastructure specs.

use std::collections::BTreeSet;

use reinhardt_cloud_types::{
	crd::{BucketSpec, InfrastructureSpec, PostgresSpec, SecretSpec},
	introspect::InfraSignals,
};
use thiserror::Error;

/// Inputs used to derive application-managed infrastructure resources.
#[derive(Debug, Clone)]
pub struct InfrastructureDerivationInput {
	/// Application name used for derived resource names.
	pub app_name: String,
	/// Infrastructure signals detected during application introspection.
	pub signals: InfraSignals,
	/// Explicit infrastructure declaration supplied by the application.
	pub explicit: Option<InfrastructureSpec>,
	/// Typed secret references detected in application settings.
	pub typed_secret_refs: Vec<String>,
}

/// Errors returned while deriving infrastructure specs.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DerivationError {
	/// Database signal requested an unsupported managed database engine.
	#[error(
		"unsupported managed database engine `{engine}`; supported values: postgres, postgresql"
	)]
	UnsupportedDatabaseEngine { engine: String },

	/// Storage signal requested an unsupported managed storage backend.
	#[error("unsupported managed storage backend `{backend}`; supported values: s3, gcs")]
	UnsupportedStorageBackend { backend: String },

	/// Derived or explicit infrastructure failed validation.
	#[error("invalid derived infrastructure: {message}")]
	InvalidInfrastructure { message: String },
}

/// Derives an infrastructure spec from introspection signals.
pub fn derive_infrastructure_spec(
	input: InfrastructureDerivationInput,
) -> Result<Option<InfrastructureSpec>, DerivationError> {
	if let Some(explicit) = input.explicit {
		validate_spec(&explicit)?;
		return Ok(Some(explicit));
	}

	let mut spec = InfrastructureSpec::default();

	if let Some(engine) = input.signals.database {
		match engine.as_str() {
			"postgres" | "postgresql" => {
				spec.postgres = Some(PostgresSpec {
					tier: None,
					version: Some("16".to_string()),
					backup_retention_days: Some(7),
				});
			}
			_ => return Err(DerivationError::UnsupportedDatabaseEngine { engine }),
		}
	}

	if let Some(backend) = input.signals.storage {
		match backend.as_str() {
			"s3" | "gcs" => {
				spec.buckets = Some(vec![BucketSpec {
					name: format!("{}-assets", input.app_name),
					public: false,
				}]);
			}
			_ => return Err(DerivationError::UnsupportedStorageBackend { backend }),
		}
	}

	let secret_names: BTreeSet<String> = input
		.typed_secret_refs
		.into_iter()
		.map(|name| name.trim().to_string())
		.filter(|name| !name.is_empty())
		.collect();
	if !secret_names.is_empty() {
		spec.secrets = Some(
			secret_names
				.into_iter()
				.map(|name| SecretSpec {
					name,
					description: Some("Application-managed secret reference".to_string()),
				})
				.collect(),
		);
	}

	if spec.postgres.is_none()
		&& spec.buckets.as_ref().is_none_or(Vec::is_empty)
		&& spec.dns.as_ref().is_none_or(Vec::is_empty)
		&& spec.secrets.as_ref().is_none_or(Vec::is_empty)
	{
		return Ok(None);
	}

	validate_spec(&spec)?;
	Ok(Some(spec))
}

fn validate_spec(spec: &InfrastructureSpec) -> Result<(), DerivationError> {
	spec.validate().map_err(|errors| {
		let message = errors
			.into_iter()
			.map(|error| error.message)
			.collect::<Vec<_>>()
			.join("; ");
		DerivationError::InvalidInfrastructure { message }
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use reinhardt_cloud_types::crd::{BucketSpec, PostgresSpec, SecretSpec};
	use rstest::rstest;

	fn input_with_signals(signals: InfraSignals) -> InfrastructureDerivationInput {
		InfrastructureDerivationInput {
			app_name: "inventory".to_string(),
			signals,
			explicit: None,
			typed_secret_refs: Vec::new(),
		}
	}

	#[rstest]
	#[case("postgres")]
	#[case("postgresql")]
	fn derives_postgres_defaults(#[case] engine: &str) {
		// Arrange
		let input = input_with_signals(InfraSignals {
			database: Some(engine.to_string()),
			..Default::default()
		});

		// Act
		let spec = derive_infrastructure_spec(input)
			.expect("postgres derivation should succeed")
			.expect("postgres signal should derive infrastructure");

		// Assert
		let postgres = spec.postgres.expect("postgres spec should be present");
		assert_eq!(postgres.version.as_deref(), Some("16"));
		assert_eq!(postgres.backup_retention_days, Some(7));
		assert_eq!(postgres.tier, None);
	}

	#[rstest]
	#[case("mysql")]
	#[case("sqlite")]
	fn rejects_unsupported_database_engines(#[case] engine: &str) {
		// Arrange
		let input = input_with_signals(InfraSignals {
			database: Some(engine.to_string()),
			..Default::default()
		});

		// Act
		let error = derive_infrastructure_spec(input).unwrap_err();

		// Assert
		assert_eq!(
			error,
			DerivationError::UnsupportedDatabaseEngine {
				engine: engine.to_string()
			}
		);
	}

	#[rstest]
	#[case("s3")]
	#[case("gcs")]
	fn derives_private_asset_bucket(#[case] backend: &str) {
		// Arrange
		let input = input_with_signals(InfraSignals {
			storage: Some(backend.to_string()),
			..Default::default()
		});

		// Act
		let spec = derive_infrastructure_spec(input)
			.expect("storage derivation should succeed")
			.expect("storage signal should derive infrastructure");

		// Assert
		assert_eq!(
			spec.buckets,
			Some(vec![BucketSpec {
				name: "inventory-assets".to_string(),
				public: false,
			}])
		);
	}

	#[rstest]
	#[case("local")]
	#[case("pvc")]
	#[case("minio")]
	fn rejects_unsupported_storage_backends(#[case] backend: &str) {
		// Arrange
		let input = input_with_signals(InfraSignals {
			storage: Some(backend.to_string()),
			..Default::default()
		});

		// Act
		let error = derive_infrastructure_spec(input).unwrap_err();

		// Assert
		assert_eq!(
			error,
			DerivationError::UnsupportedStorageBackend {
				backend: backend.to_string()
			}
		);
	}

	#[rstest]
	fn returns_none_without_managed_signals_or_typed_secret_refs() {
		// Arrange
		let input = input_with_signals(InfraSignals::default());

		// Act
		let spec = derive_infrastructure_spec(input).expect("empty derivation should succeed");

		// Assert
		assert_eq!(spec, None);
	}

	#[rstest]
	fn preserves_explicit_infrastructure_after_validation() {
		// Arrange
		let explicit = InfrastructureSpec {
			postgres: Some(PostgresSpec {
				tier: Some("db-custom-2-4096".to_string()),
				version: Some("15".to_string()),
				backup_retention_days: Some(14),
			}),
			buckets: Some(vec![BucketSpec {
				name: "custom-assets".to_string(),
				public: true,
			}]),
			dns: None,
			secrets: Some(vec![SecretSpec {
				name: "api-token".to_string(),
				description: Some("Provided by operator".to_string()),
			}]),
		};
		let input = InfrastructureDerivationInput {
			app_name: "inventory".to_string(),
			signals: InfraSignals {
				database: Some("postgres".to_string()),
				storage: Some("s3".to_string()),
				..Default::default()
			},
			explicit: Some(explicit.clone()),
			typed_secret_refs: vec!["other-secret".to_string()],
		};

		// Act
		let spec = derive_infrastructure_spec(input).expect("explicit spec should be valid");

		// Assert
		assert_eq!(spec, Some(explicit));
	}

	#[rstest]
	fn derives_sorted_deduplicated_typed_secret_refs() {
		// Arrange
		let input = InfrastructureDerivationInput {
			app_name: "inventory".to_string(),
			signals: InfraSignals::default(),
			explicit: None,
			typed_secret_refs: vec![
				"stripe-api-key".to_string(),
				" ".to_string(),
				"database-url".to_string(),
				"stripe-api-key".to_string(),
			],
		};

		// Act
		let spec = derive_infrastructure_spec(input)
			.expect("secret derivation should succeed")
			.expect("secret refs should derive infrastructure");

		// Assert
		assert_eq!(
			spec.secrets,
			Some(vec![
				SecretSpec {
					name: "database-url".to_string(),
					description: Some("Application-managed secret reference".to_string()),
				},
				SecretSpec {
					name: "stripe-api-key".to_string(),
					description: Some("Application-managed secret reference".to_string()),
				},
			])
		);
	}
}
