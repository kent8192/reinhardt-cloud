use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::database::ResourcePhase;

/// Cache infrastructure specification
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CacheSpec {
	/// Cache backend type
	#[serde(default)]
	pub backend: CacheBackend,
	/// Instance type (platform-specific)
	pub instance_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CacheBackend {
	#[default]
	Redis,
}

/// Status of a provisioned cache resource
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct CacheStatus {
	pub phase: ResourcePhase,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub endpoint: Option<String>,
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_cache_backend_default_is_redis() {
		// Arrange & Act
		let backend = CacheBackend::default();
		// Assert
		assert_eq!(backend, CacheBackend::Redis);
	}

	#[rstest]
	fn test_cache_spec_serialization_roundtrip() {
		// Arrange
		let spec = CacheSpec {
			backend: CacheBackend::Redis,
			instance_type: Some("cache.t3.micro".to_string()),
		};
		// Act
		let json = serde_json::to_string(&spec).unwrap();
		let deserialized: CacheSpec = serde_json::from_str(&json).unwrap();
		// Assert
		assert_eq!(deserialized.backend, CacheBackend::Redis);
		assert_eq!(deserialized.instance_type, Some("cache.t3.micro".to_string()));
	}
}
