//! Workload isolation types for security boundaries.
//!
//! Defines isolation levels and network policies that control
//! how `ReinhardtApp` workloads are sandboxed from each other.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::validation::ValidationError;

/// Helper for serde default values that default to `true`.
fn default_true() -> bool {
	true
}

/// Workload isolation level for security boundaries.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub enum IsolationLevel {
	/// Standard container isolation (Linux namespaces + cgroups only)
	#[default]
	None,
	/// Userspace kernel sandbox (gVisor/runsc)
	Sandbox,
	/// Hardware-virtualized microVM (Kata Containers + Cloud Hypervisor)
	MicroVM,
}

/// Network isolation policy for the application.
///
/// Default behavior when specified as `network: {}` in YAML:
/// - `block_metadata_service`: true (IMDS blocked)
/// - `allow_egress`: true (external traffic allowed)
/// - `egress_allow_cidrs`: empty (all non-IMDS CIDRs allowed)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct NetworkIsolationSpec {
	/// Block access to cloud metadata service (169.254.169.254).
	/// Defaults to true.
	#[serde(default = "default_true")]
	pub block_metadata_service: bool,
	/// Allow egress to external networks.
	/// Defaults to true.
	#[serde(default = "default_true")]
	pub allow_egress: bool,
	/// Allowed egress CIDR blocks (only effective when `allow_egress` is true).
	#[serde(default)]
	pub egress_allow_cidrs: Vec<String>,
}

impl Default for NetworkIsolationSpec {
	fn default() -> Self {
		Self {
			block_metadata_service: true,
			allow_egress: true,
			egress_allow_cidrs: Vec::new(),
		}
	}
}

impl NetworkIsolationSpec {
	/// Validates the network isolation specification.
	///
	/// Checks that all `egress_allow_cidrs` entries are valid CIDR notation
	/// (must contain a `/` separator).
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		for (i, cidr) in self.egress_allow_cidrs.iter().enumerate() {
			if !cidr.contains('/') {
				errors.push(ValidationError::new(format!(
					"isolation.network.egress_allow_cidrs[{i}] is not valid CIDR: {cidr}"
				)));
			}
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

/// Workload isolation and security configuration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct IsolationSpec {
	/// Desired isolation level.
	#[serde(default)]
	pub level: IsolationLevel,
	/// Network isolation policy.
	pub network: Option<NetworkIsolationSpec>,
	/// Override the RuntimeClass name (advanced usage).
	/// Must be non-empty when specified.
	pub runtime_class_override: Option<String>,
}

impl IsolationSpec {
	/// Validates the isolation specification.
	///
	/// Checks that `runtime_class_override` is non-empty when set,
	/// and delegates to `NetworkIsolationSpec::validate()`.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(ref name) = self.runtime_class_override
			&& name.is_empty()
		{
			errors.push(ValidationError::new(
				"isolation.runtime_class_override must be non-empty when specified",
			));
		}

		if let Some(ref network) = self.network
			&& let Err(errs) = network.validate()
		{
			errors.extend(errs);
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
	fn isolation_level_default_is_none() {
		// Arrange & Act
		let level = IsolationLevel::default();

		// Assert
		assert_eq!(level, IsolationLevel::None);
	}

	#[rstest]
	fn isolation_level_serialization_roundtrip() {
		// Arrange
		let levels = vec![
			IsolationLevel::None,
			IsolationLevel::Sandbox,
			IsolationLevel::MicroVM,
		];

		for level in levels {
			// Act
			let json = serde_json::to_string(&level).unwrap();
			let deserialized: IsolationLevel = serde_json::from_str(&json).unwrap();

			// Assert
			assert_eq!(deserialized, level);
		}
	}

	#[rstest]
	fn network_isolation_spec_defaults() {
		// Arrange
		let json = r#"{}"#;

		// Act
		let spec: NetworkIsolationSpec = serde_json::from_str(json).unwrap();

		// Assert
		assert!(spec.block_metadata_service);
		assert!(spec.allow_egress);
		assert!(spec.egress_allow_cidrs.is_empty());
	}

	#[rstest]
	fn network_isolation_spec_default_trait() {
		// Arrange & Act
		let spec = NetworkIsolationSpec::default();

		// Assert
		assert!(spec.block_metadata_service);
		assert!(spec.allow_egress);
		assert!(spec.egress_allow_cidrs.is_empty());
	}

	#[rstest]
	fn network_isolation_spec_validates_valid_cidrs() {
		// Arrange
		let spec = NetworkIsolationSpec {
			egress_allow_cidrs: vec![
				"10.0.0.0/8".to_string(),
				"172.16.0.0/12".to_string(),
			],
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn network_isolation_spec_rejects_invalid_cidrs() {
		// Arrange
		let spec = NetworkIsolationSpec {
			egress_allow_cidrs: vec![
				"10.0.0.0/8".to_string(),
				"not-a-cidr".to_string(),
			],
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert!(errors[0].message.contains("not-a-cidr"));
	}

	#[rstest]
	fn isolation_spec_default_is_none_level() {
		// Arrange & Act
		let spec = IsolationSpec::default();

		// Assert
		assert_eq!(spec.level, IsolationLevel::None);
		assert!(spec.network.is_none());
		assert!(spec.runtime_class_override.is_none());
	}

	#[rstest]
	fn isolation_spec_validates_empty_override() {
		// Arrange
		let spec = IsolationSpec {
			runtime_class_override: Some(String::new()),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert!(errors[0].message.contains("non-empty"));
	}

	#[rstest]
	fn isolation_spec_validates_valid_override() {
		// Arrange
		let spec = IsolationSpec {
			runtime_class_override: Some("kata-fc".to_string()),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn isolation_spec_delegates_network_validation() {
		// Arrange
		let spec = IsolationSpec {
			network: Some(NetworkIsolationSpec {
				egress_allow_cidrs: vec!["bad-cidr".to_string()],
				..Default::default()
			}),
			..Default::default()
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert!(errors[0].message.contains("bad-cidr"));
	}

	#[rstest]
	fn isolation_spec_full_roundtrip() {
		// Arrange
		let spec = IsolationSpec {
			level: IsolationLevel::MicroVM,
			network: Some(NetworkIsolationSpec {
				block_metadata_service: true,
				allow_egress: true,
				egress_allow_cidrs: vec!["10.0.0.0/8".to_string()],
			}),
			runtime_class_override: None,
		};

		// Act
		let json = serde_json::to_string(&spec).unwrap();
		let deserialized: IsolationSpec = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(deserialized, spec);
	}
}
