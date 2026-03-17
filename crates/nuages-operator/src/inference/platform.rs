//! Platform configuration for the inference engine.
//!
//! Provides per-platform default values (AWS, GCP, on-premise) that drive
//! resource inference when users omit optional spec fields.

use serde::{Deserialize, Serialize};

/// Target deployment platform.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Platform {
	Aws,
	Gcp,
	Onpremise,
}

/// Platform-specific configuration loaded from Helm values.
#[derive(Debug, Clone)]
pub(crate) struct PlatformConfig {
	pub platform: Platform,
	pub defaults: PlatformDefaults,
}

/// Aggregated platform defaults for all resource categories.
#[derive(Debug, Clone)]
pub(crate) struct PlatformDefaults {
	pub database: DatabaseDefaults,
	pub cache: CacheDefaults,
	pub resources: ResourceDefaults,
}

/// Database provisioning defaults.
#[derive(Debug, Clone)]
pub(crate) struct DatabaseDefaults {
	pub instance_class: String,
	pub storage_gb: i32,
}

/// Cache provisioning defaults.
#[derive(Debug, Clone)]
pub(crate) struct CacheDefaults {
	pub instance_type: String,
}

/// Container resource request/limit defaults.
#[derive(Debug, Clone)]
pub(crate) struct ResourceDefaults {
	pub cpu_request: String,
	pub memory_request: String,
	pub cpu_limit: String,
	pub memory_limit: String,
}

impl PlatformConfig {
	/// Detect the platform from the `NUAGES_PLATFORM` environment variable
	/// and return the corresponding default configuration.
	///
	/// Falls back to on-premise defaults when the variable is unset or
	/// contains an unrecognised value.
	pub(crate) fn from_env() -> Self {
		let platform_str =
			std::env::var("NUAGES_PLATFORM").unwrap_or_else(|_| "onpremise".to_string());
		match platform_str.as_str() {
			"aws" => Self::aws_defaults(),
			"gcp" => Self::gcp_defaults(),
			_ => Self::onprem_defaults(),
		}
	}

	/// AWS (EKS) platform defaults.
	///
	/// Database: RDS db.t3.micro, 20 GB storage.
	/// Cache: ElastiCache cache.t3.micro.
	/// Resources: 100m/128Mi requests, 500m/512Mi limits.
	pub(crate) fn aws_defaults() -> Self {
		Self {
			platform: Platform::Aws,
			defaults: PlatformDefaults {
				database: DatabaseDefaults {
					instance_class: "db.t3.micro".to_string(),
					storage_gb: 20,
				},
				cache: CacheDefaults {
					instance_type: "cache.t3.micro".to_string(),
				},
				resources: ResourceDefaults {
					cpu_request: "100m".to_string(),
					memory_request: "128Mi".to_string(),
					cpu_limit: "500m".to_string(),
					memory_limit: "512Mi".to_string(),
				},
			},
		}
	}

	/// GCP (GKE) platform defaults.
	///
	/// Database: Cloud SQL db-f1-micro, 10 GB storage.
	/// Cache: Memorystore BASIC tier.
	/// Resources: 100m/128Mi requests, 500m/512Mi limits.
	pub(crate) fn gcp_defaults() -> Self {
		Self {
			platform: Platform::Gcp,
			defaults: PlatformDefaults {
				database: DatabaseDefaults {
					instance_class: "db-f1-micro".to_string(),
					storage_gb: 10,
				},
				cache: CacheDefaults {
					instance_type: "BASIC".to_string(),
				},
				resources: ResourceDefaults {
					cpu_request: "100m".to_string(),
					memory_request: "128Mi".to_string(),
					cpu_limit: "500m".to_string(),
					memory_limit: "512Mi".to_string(),
				},
			},
		}
	}

	/// On-premise platform defaults.
	///
	/// Database: local StatefulSet, 20 GB storage (no cloud instance class).
	/// Cache: local Redis StatefulSet (no cloud instance type).
	/// Resources: 100m/128Mi requests, 1000m/1Gi limits.
	pub(crate) fn onprem_defaults() -> Self {
		Self {
			platform: Platform::Onpremise,
			defaults: PlatformDefaults {
				database: DatabaseDefaults {
					instance_class: String::new(),
					storage_gb: 20,
				},
				cache: CacheDefaults {
					instance_type: String::new(),
				},
				resources: ResourceDefaults {
					cpu_request: "100m".to_string(),
					memory_request: "128Mi".to_string(),
					cpu_limit: "1000m".to_string(),
					memory_limit: "1Gi".to_string(),
				},
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn aws_defaults_returns_correct_platform() {
		// Arrange & Act
		let config = PlatformConfig::aws_defaults();

		// Assert
		assert_eq!(config.platform, Platform::Aws);
	}

	#[rstest]
	fn aws_defaults_has_correct_database_values() {
		// Arrange & Act
		let config = PlatformConfig::aws_defaults();

		// Assert
		assert_eq!(config.defaults.database.instance_class, "db.t3.micro");
		assert_eq!(config.defaults.database.storage_gb, 20);
	}

	#[rstest]
	fn aws_defaults_has_correct_cache_values() {
		// Arrange & Act
		let config = PlatformConfig::aws_defaults();

		// Assert
		assert_eq!(config.defaults.cache.instance_type, "cache.t3.micro");
	}

	#[rstest]
	fn aws_defaults_has_correct_resource_values() {
		// Arrange & Act
		let config = PlatformConfig::aws_defaults();

		// Assert
		assert_eq!(config.defaults.resources.cpu_request, "100m");
		assert_eq!(config.defaults.resources.memory_request, "128Mi");
		assert_eq!(config.defaults.resources.cpu_limit, "500m");
		assert_eq!(config.defaults.resources.memory_limit, "512Mi");
	}

	#[rstest]
	fn gcp_defaults_returns_correct_platform() {
		// Arrange & Act
		let config = PlatformConfig::gcp_defaults();

		// Assert
		assert_eq!(config.platform, Platform::Gcp);
	}

	#[rstest]
	fn gcp_defaults_has_correct_database_values() {
		// Arrange & Act
		let config = PlatformConfig::gcp_defaults();

		// Assert
		assert_eq!(config.defaults.database.instance_class, "db-f1-micro");
		assert_eq!(config.defaults.database.storage_gb, 10);
	}

	#[rstest]
	fn gcp_defaults_has_correct_cache_values() {
		// Arrange & Act
		let config = PlatformConfig::gcp_defaults();

		// Assert
		assert_eq!(config.defaults.cache.instance_type, "BASIC");
	}

	#[rstest]
	fn onprem_defaults_returns_correct_platform() {
		// Arrange & Act
		let config = PlatformConfig::onprem_defaults();

		// Assert
		assert_eq!(config.platform, Platform::Onpremise);
	}

	#[rstest]
	fn onprem_defaults_has_empty_instance_class() {
		// Arrange & Act
		let config = PlatformConfig::onprem_defaults();

		// Assert
		assert!(config.defaults.database.instance_class.is_empty());
		assert!(config.defaults.cache.instance_type.is_empty());
	}

	#[rstest]
	fn onprem_defaults_has_higher_resource_limits() {
		// Arrange & Act
		let config = PlatformConfig::onprem_defaults();

		// Assert
		assert_eq!(config.defaults.resources.cpu_limit, "1000m");
		assert_eq!(config.defaults.resources.memory_limit, "1Gi");
	}

	#[rstest]
	fn onprem_defaults_has_20gb_storage() {
		// Arrange & Act
		let config = PlatformConfig::onprem_defaults();

		// Assert
		assert_eq!(config.defaults.database.storage_gb, 20);
	}

	#[rstest]
	fn from_env_defaults_to_onpremise_when_unset() {
		// Arrange
		// SAFETY: test-only env manipulation; tests using env vars
		// must run with `#[serial]` if parallelism is a concern.
		unsafe {
			std::env::remove_var("NUAGES_PLATFORM");
		}

		// Act
		let config = PlatformConfig::from_env();

		// Assert
		assert_eq!(config.platform, Platform::Onpremise);
	}

	#[rstest]
	fn from_env_selects_aws() {
		// Arrange
		// SAFETY: test-only env manipulation
		unsafe {
			std::env::set_var("NUAGES_PLATFORM", "aws");
		}

		// Act
		let config = PlatformConfig::from_env();

		// Assert
		assert_eq!(config.platform, Platform::Aws);

		// Cleanup
		unsafe {
			std::env::remove_var("NUAGES_PLATFORM");
		}
	}

	#[rstest]
	fn from_env_selects_gcp() {
		// Arrange
		// SAFETY: test-only env manipulation
		unsafe {
			std::env::set_var("NUAGES_PLATFORM", "gcp");
		}

		// Act
		let config = PlatformConfig::from_env();

		// Assert
		assert_eq!(config.platform, Platform::Gcp);

		// Cleanup
		unsafe {
			std::env::remove_var("NUAGES_PLATFORM");
		}
	}

	#[rstest]
	fn from_env_falls_back_to_onpremise_for_unknown() {
		// Arrange
		// SAFETY: test-only env manipulation
		unsafe {
			std::env::set_var("NUAGES_PLATFORM", "azure");
		}

		// Act
		let config = PlatformConfig::from_env();

		// Assert
		assert_eq!(config.platform, Platform::Onpremise);

		// Cleanup
		unsafe {
			std::env::remove_var("NUAGES_PLATFORM");
		}
	}

	#[rstest]
	fn platform_serializes_lowercase() {
		// Arrange
		let platform = Platform::Aws;

		// Act
		let json = serde_json::to_string(&platform).unwrap();

		// Assert
		assert_eq!(json, r#""aws""#);
	}

	#[rstest]
	fn platform_deserializes_lowercase() {
		// Arrange
		let json = r#""gcp""#;

		// Act
		let platform: Platform = serde_json::from_str(json).unwrap();

		// Assert
		assert_eq!(platform, Platform::Gcp);
	}
}
