use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Maximum database storage tenants may request through `spec.database`.
pub const MAX_DATABASE_STORAGE_GB: i32 = 100;

/// Safe database instance classes accepted for managed cloud databases.
pub const ALLOWED_DATABASE_INSTANCE_CLASSES: &[&str] = &[
	"db.t3.micro",
	"db.t3.small",
	"db.t3.medium",
	"db.t3.large",
	"db.r5.large",
	"db-f1-micro",
	"db-g1-small",
	"db-custom-1-3840",
	"db-custom-2-8192",
];

/// Database infrastructure specification
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DatabaseSpec {
	pub engine: DatabaseEngine,
	/// Instance class (platform-specific, e.g., "db.t3.micro").
	/// Defaults to platform profile value if unset.
	///
	/// Explicit values must be one of `ALLOWED_DATABASE_INSTANCE_CLASSES`.
	pub instance_class: Option<String>,
	/// Storage size in GB. Must be between 1 and `MAX_DATABASE_STORAGE_GB`.
	pub storage_gb: Option<i32>,
	/// Engine version (e.g., "16" for PostgreSQL).
	pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseEngine {
	Postgresql,
	Mysql,
	Sqlite,
}

/// Status of a provisioned database resource
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct DatabaseStatus {
	/// Provisioning phase
	pub phase: ResourcePhase,
	/// Connection endpoint (host:port)
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub endpoint: Option<String>,
	/// Secret name containing credentials
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub credentials_secret: Option<String>,
}

/// Phase of a sub-resource (database, cache, etc.)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResourcePhase {
	#[default]
	Pending,
	Provisioning,
	Ready,
	Failed,
}

impl DatabaseSpec {
	/// Validates the database specification
	pub fn validate(&self) -> Result<(), String> {
		if let Some(gb) = self.storage_gb {
			if gb <= 0 {
				return Err("database.storage_gb must be > 0".to_string());
			}
			if gb > MAX_DATABASE_STORAGE_GB {
				return Err(format!(
					"database.storage_gb must be <= {MAX_DATABASE_STORAGE_GB}"
				));
			}
		}

		if let Some(instance_class) = self.instance_class.as_deref()
			&& !ALLOWED_DATABASE_INSTANCE_CLASSES.contains(&instance_class)
		{
			return Err(format!(
				"database.instance_class must be one of: {}",
				ALLOWED_DATABASE_INSTANCE_CLASSES.join(", ")
			));
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_database_spec_valid() {
		// Arrange
		let spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: Some("db.t3.micro".to_string()),
			storage_gb: Some(20),
			version: Some("16".to_string()),
		};
		// Act
		let result = spec.validate();
		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn test_database_spec_rejects_negative_storage() {
		// Arrange
		let spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: Some(-1),
			version: None,
		};
		// Act
		let result = spec.validate();
		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_database_spec_rejects_zero_storage() {
		// Arrange
		let spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: Some(0),
			version: None,
		};
		// Act
		let result = spec.validate();
		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_database_spec_rejects_excessive_storage() {
		// Arrange
		let spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: None,
			storage_gb: Some(MAX_DATABASE_STORAGE_GB + 1),
			version: None,
		};

		// Act
		let result = spec.validate();

		// Assert
		assert_eq!(
			result,
			Err(format!(
				"database.storage_gb must be <= {MAX_DATABASE_STORAGE_GB}"
			))
		);
	}

	#[rstest]
	fn test_database_spec_rejects_unsafe_instance_class() {
		// Arrange
		let spec = DatabaseSpec {
			engine: DatabaseEngine::Postgresql,
			instance_class: Some("db.r7i.48xlarge".to_string()),
			storage_gb: Some(20),
			version: None,
		};

		// Act
		let result = spec.validate();

		// Assert
		assert_eq!(
			result,
			Err(format!(
				"database.instance_class must be one of: {}",
				ALLOWED_DATABASE_INSTANCE_CLASSES.join(", ")
			))
		);
	}

	#[rstest]
	fn test_database_spec_allows_none_storage() {
		// Arrange
		let spec = DatabaseSpec {
			engine: DatabaseEngine::Mysql,
			instance_class: None,
			storage_gb: None,
			version: None,
		};
		// Act
		let result = spec.validate();
		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn test_database_engine_serializes_lowercase() {
		// Arrange
		let engine = DatabaseEngine::Postgresql;
		// Act
		let json = serde_json::to_string(&engine).unwrap();
		// Assert
		assert_eq!(json, r#""postgresql""#);
	}

	#[rstest]
	fn test_resource_phase_default_is_pending() {
		// Arrange & Act
		let phase = ResourcePhase::default();
		// Assert
		assert_eq!(phase, ResourcePhase::Pending);
	}
}
