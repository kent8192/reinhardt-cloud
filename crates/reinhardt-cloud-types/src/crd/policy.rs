use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Controls what happens to cloud resources when `Project` is deleted.
/// - `Retain` (default): cloud resources are kept for manual cleanup
/// - `Delete`: cloud resources are deleted (DATA LOSS risk)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DeletionPolicy {
	#[default]
	Retain,
	Delete,
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_deletion_policy_default_is_retain() {
		// Arrange & Act
		let policy = DeletionPolicy::default();
		// Assert
		assert_eq!(policy, DeletionPolicy::Retain);
	}

	#[rstest]
	fn test_deletion_policy_serialization() {
		// Arrange
		let retain = DeletionPolicy::Retain;
		let delete = DeletionPolicy::Delete;
		// Act
		let retain_json = serde_json::to_string(&retain).unwrap();
		let delete_json = serde_json::to_string(&delete).unwrap();
		// Assert
		assert_eq!(retain_json, r#""retain""#);
		assert_eq!(delete_json, r#""delete""#);
	}

	#[rstest]
	fn test_deletion_policy_deserialization() {
		// Arrange
		let json = r#""retain""#;
		// Act
		let policy: DeletionPolicy = serde_json::from_str(json).unwrap();
		// Assert
		assert_eq!(policy, DeletionPolicy::Retain);
	}
}
