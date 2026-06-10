//! Per-application managed cloud infrastructure types.
//!
//! These types extend `ProjectSpec` with an `infrastructure` block
//! that declares cloud-managed resources (Postgres, object storage, DNS
//! records, secrets) needed by the application. The `terraform generate`
//! CLI subcommand reads this block and emits provider-scoped HCL.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::validation::ValidationError;

/// Postgres tier / size declaration for a per-app managed database.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub struct PostgresSpec {
	/// Provider-specific tier or instance class.
	///
	/// GCP example: `"db-custom-2-4096"`. AWS example: `"db.t3.micro"`.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub tier: Option<String>,

	/// PostgreSQL major version (e.g., `"16"`).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub version: Option<String>,

	/// Number of days to retain automated backups.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub backup_retention_days: Option<u32>,
}

impl PostgresSpec {
	/// Validates the Postgres spec.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(days) = self.backup_retention_days
			&& days == 0
		{
			errors.push(ValidationError::new(
				"infrastructure.postgres.backup_retention_days must be > 0",
			));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

/// Object storage bucket declaration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct BucketSpec {
	/// Logical bucket name. The operator appends a namespace-scoped suffix
	/// to prevent collisions across tenants.
	pub name: String,

	/// When `true` the bucket allows anonymous read access (e.g., for
	/// static asset hosting). Defaults to `false`.
	#[serde(default)]
	pub public: bool,
}

impl BucketSpec {
	/// Validates the bucket spec.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if self.name.trim().is_empty() {
			errors.push(ValidationError::new(
				"infrastructure.buckets[].name must be non-empty",
			));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

/// DNS record declaration (Cloud DNS / Route 53).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct DnsRecordSpec {
	/// Fully-qualified hostname (e.g., `"orders.acme.example.com"`).
	pub host: String,

	/// DNS record type. Supported values: `"A"`, `"CNAME"`, `"TXT"`.
	#[serde(rename = "type")]
	pub record_type: String,
}

impl DnsRecordSpec {
	/// Validates the DNS record spec.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if self.host.trim().is_empty() {
			errors.push(ValidationError::new(
				"infrastructure.dns[].host must be non-empty",
			));
		}

		let valid_types = ["A", "CNAME", "TXT"];
		if !valid_types.contains(&self.record_type.as_str()) {
			errors.push(ValidationError::new(format!(
				"infrastructure.dns[].type must be one of {:?}",
				valid_types
			)));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

/// Application-scoped secret declaration (Secret Manager / AWS Secrets Manager).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct SecretSpec {
	/// Logical secret name. Combined with namespace and app name to form the
	/// provider-specific secret path.
	pub name: String,

	/// Optional human-readable description stored as secret metadata.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
}

impl SecretSpec {
	/// Validates the secret spec.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if self.name.trim().is_empty() {
			errors.push(ValidationError::new(
				"infrastructure.secrets[].name must be non-empty",
			));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

/// Per-application managed cloud resources declared on `ProjectSpec`.
///
/// Each field is optional; only declared resources are provisioned. The
/// `terraform generate` CLI subcommand translates this block into
/// provider-scoped HCL targeting the cluster's configured provider
/// (GCP or AWS).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Default)]
pub struct InfrastructureSpec {
	/// Managed Postgres database (Cloud SQL / RDS).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub postgres: Option<PostgresSpec>,

	/// Object storage buckets (GCS / S3).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub buckets: Option<Vec<BucketSpec>>,

	/// DNS records (Cloud DNS / Route 53).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub dns: Option<Vec<DnsRecordSpec>>,

	/// Application-scoped secrets (Secret Manager / AWS Secrets Manager).
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub secrets: Option<Vec<SecretSpec>>,
}

impl InfrastructureSpec {
	/// Validates the infrastructure spec and all nested resources.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(ref pg) = self.postgres
			&& let Err(errs) = pg.validate()
		{
			errors.extend(errs);
		}

		if let Some(ref buckets) = self.buckets {
			for bucket in buckets {
				if let Err(errs) = bucket.validate() {
					errors.extend(errs);
				}
			}
		}

		if let Some(ref dns) = self.dns {
			for record in dns {
				if let Err(errs) = record.validate() {
					errors.extend(errs);
				}
			}
		}

		if let Some(ref secrets) = self.secrets {
			for secret in secrets {
				if let Err(errs) = secret.validate() {
					errors.extend(errs);
				}
			}
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn infrastructure_spec_defaults_all_none() {
		// Arrange
		let json = "{}";

		// Act
		let spec: InfrastructureSpec =
			serde_json::from_str(json).expect("deserialization should succeed");

		// Assert
		assert!(spec.postgres.is_none());
		assert!(spec.buckets.is_none());
		assert!(spec.dns.is_none());
		assert!(spec.secrets.is_none());
	}

	#[rstest]
	fn infrastructure_spec_roundtrip_with_postgres() {
		// Arrange
		let spec = InfrastructureSpec {
			postgres: Some(PostgresSpec {
				tier: Some("db-custom-2-4096".to_string()),
				version: Some("16".to_string()),
				backup_retention_days: Some(7),
			}),
			buckets: None,
			dns: None,
			secrets: None,
		};

		// Act
		let json = serde_json::to_string(&spec).expect("serialization should succeed");
		let deserialized: InfrastructureSpec =
			serde_json::from_str(&json).expect("deserialization should succeed");

		// Assert
		let pg = deserialized.postgres.unwrap();
		assert_eq!(pg.tier.as_deref(), Some("db-custom-2-4096"));
		assert_eq!(pg.version.as_deref(), Some("16"));
		assert_eq!(pg.backup_retention_days, Some(7));
	}

	#[rstest]
	fn postgres_spec_rejects_zero_retention() {
		// Arrange
		let spec = PostgresSpec {
			backup_retention_days: Some(0),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert!(
			errors[0]
				.message
				.contains("backup_retention_days must be > 0")
		);
	}

	#[rstest]
	fn bucket_spec_rejects_empty_name() {
		// Arrange
		let spec = BucketSpec {
			name: "  ".to_string(),
			public: false,
		};

		// Act
		let errors = spec.validate().unwrap_err();

		// Assert
		assert!(errors[0].message.contains("name must be non-empty"));
	}

	#[rstest]
	fn dns_record_spec_rejects_invalid_type() {
		// Arrange
		let spec = DnsRecordSpec {
			host: "orders.example.com".to_string(),
			record_type: "MX".to_string(),
		};

		// Act
		let errors = spec.validate().unwrap_err();

		// Assert
		assert!(errors[0].message.contains("type must be one of"));
	}

	#[rstest]
	fn dns_record_spec_accepts_valid_types() {
		// Arrange
		let types = ["A", "CNAME", "TXT"];

		for t in types {
			// Act
			let spec = DnsRecordSpec {
				host: "example.com".to_string(),
				record_type: t.to_string(),
			};
			let result = spec.validate();

			// Assert
			assert!(result.is_ok(), "expected {t} to be valid");
		}
	}

	#[rstest]
	fn secret_spec_rejects_empty_name() {
		// Arrange
		let spec = SecretSpec {
			name: String::new(),
			description: None,
		};

		// Act
		let errors = spec.validate().unwrap_err();

		// Assert
		assert!(errors[0].message.contains("name must be non-empty"));
	}

	#[rstest]
	fn infrastructure_spec_validates_all_nested() {
		// Arrange
		let spec = InfrastructureSpec {
			postgres: Some(PostgresSpec {
				backup_retention_days: Some(0),
				..Default::default()
			}),
			buckets: Some(vec![BucketSpec {
				name: String::new(),
				public: false,
			}]),
			dns: Some(vec![DnsRecordSpec {
				host: String::new(),
				record_type: "INVALID".to_string(),
			}]),
			secrets: Some(vec![SecretSpec {
				name: String::new(),
				description: None,
			}]),
		};

		// Act
		let errors = spec.validate().unwrap_err();

		// Assert: postgres(1) + bucket(1) + dns host empty(1) + dns type invalid(1) + secret(1) = 5
		assert_eq!(errors.len(), 5);
	}
}
