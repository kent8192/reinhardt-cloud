//! Per-app ServiceAccount configuration for `ReinhardtApp`.
//!
//! This is distinct from the operator-managed `{app-name}-storage` KSA used
//! solely for storage-backend access. The `ServiceAccountSpec` configures the
//! application workload's KSA, typically annotated with GKE Workload Identity
//! or AWS IRSA bindings for cloud-API access.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::validation::ValidationError;

/// Per-app ServiceAccount configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct ServiceAccountSpec {
	/// Whether the operator should create a KSA. If false, the user must pre-create one.
	#[serde(default)]
	pub create: bool,

	/// Name of the KSA. Defaults to the ReinhardtApp name when not set.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub name: Option<String>,

	/// Annotations applied to the KSA — typically Workload Identity / IRSA bindings.
	#[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
	pub annotations: BTreeMap<String, String>,
}

impl ServiceAccountSpec {
	/// Validates the ServiceAccount specification.
	///
	/// Checks that the KSA name (when set) is non-empty after trimming.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(ref name) = self.name
			&& name.trim().is_empty()
		{
			errors.push(ValidationError::new(
				"serviceAccount.name must be non-empty when set",
			));
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
	fn test_default_values() {
		// Arrange / Act
		let spec = ServiceAccountSpec::default();

		// Assert
		assert!(!spec.create);
		assert_eq!(spec.name, None);
		assert!(spec.annotations.is_empty());
	}

	#[rstest]
	fn test_roundtrip_with_annotations() {
		// Arrange
		let spec = ServiceAccountSpec {
			create: true,
			name: Some("my-app".to_string()),
			annotations: BTreeMap::from([
				(
					"iam.gke.io/gcp-service-account".to_string(),
					"my-app@project.iam.gserviceaccount.com".to_string(),
				),
				(
					"eks.amazonaws.com/role-arn".to_string(),
					"arn:aws:iam::123456789012:role/my-app".to_string(),
				),
			]),
		};

		// Act
		let json = serde_json::to_string(&spec).expect("serialization should succeed");
		let deserialized: ServiceAccountSpec =
			serde_json::from_str(&json).expect("deserialization should succeed");

		// Assert
		assert!(deserialized.create);
		assert_eq!(deserialized.name.as_deref(), Some("my-app"));
		assert_eq!(deserialized.annotations.len(), 2);
		assert_eq!(
			deserialized
				.annotations
				.get("iam.gke.io/gcp-service-account")
				.map(String::as_str),
			Some("my-app@project.iam.gserviceaccount.com"),
		);
		assert_eq!(
			deserialized
				.annotations
				.get("eks.amazonaws.com/role-arn")
				.map(String::as_str),
			Some("arn:aws:iam::123456789012:role/my-app"),
		);
	}

	#[rstest]
	fn test_validation_rejects_empty_name() {
		// Arrange
		let spec = ServiceAccountSpec {
			name: Some(String::new()),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.expect_err("empty name should fail validation");
		assert_eq!(errors.len(), 1);
		assert!(errors[0].message.contains("non-empty"));
	}

	#[rstest]
	fn test_validation_rejects_whitespace_only_name() {
		// Arrange
		let spec = ServiceAccountSpec {
			name: Some("   ".to_string()),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.expect_err("whitespace-only name should fail validation");
		assert_eq!(errors.len(), 1);
		assert!(errors[0].message.contains("non-empty"));
	}

	#[rstest]
	fn test_validation_accepts_none_name() {
		// Arrange
		let spec = ServiceAccountSpec {
			name: None,
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_ok());
	}
}
