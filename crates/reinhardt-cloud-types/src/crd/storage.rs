use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Object storage specification
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct StorageSpec {
	/// Storage backend (inferred from platform if unset)
	pub backend: Option<StorageBackend>,
	/// Bucket/volume name
	pub bucket: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StorageBackend {
	S3,
	Gcs,
	Pvc,
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_storage_spec_serialization_roundtrip() {
		// Arrange
		let spec = StorageSpec {
			backend: Some(StorageBackend::S3),
			bucket: Some("my-bucket".to_string()),
		};
		// Act
		let json = serde_json::to_string(&spec).unwrap();
		let deserialized: StorageSpec = serde_json::from_str(&json).unwrap();
		// Assert
		assert_eq!(deserialized.backend, Some(StorageBackend::S3));
		assert_eq!(deserialized.bucket, Some("my-bucket".to_string()));
	}

	#[rstest]
	fn test_storage_backend_serializes_lowercase() {
		// Arrange
		let backends = [StorageBackend::S3, StorageBackend::Gcs, StorageBackend::Pvc];
		// Act
		let jsons: Vec<String> = backends
			.iter()
			.map(|b| serde_json::to_string(b).unwrap())
			.collect();
		// Assert
		assert_eq!(jsons[0], r#""s3""#);
		assert_eq!(jsons[1], r#""gcs""#);
		assert_eq!(jsons[2], r#""pvc""#);
	}
}
