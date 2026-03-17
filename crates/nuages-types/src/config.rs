//! Configuration schema types for `reinhardt.toml`.
//!
//! Defines the structure of the `reinhardt.toml` configuration file
//! used to declare application deployment settings.

use serde::{Deserialize, Serialize};

use crate::validation::ValidationError;

/// Top-level `reinhardt.toml` configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReinhardtConfig {
	/// Application metadata
	pub app: AppConfig,
	/// Build configuration
	pub build: Option<BuildConfig>,
	/// Deployment configuration
	pub deploy: Option<DeployConfig>,
	/// Autoscaling configuration
	pub scale: Option<ScaleConfig>,
	/// Service exposure configuration
	pub services: Option<ServicesConfig>,
	/// Health check configuration
	pub health: Option<HealthConfig>,
}

/// Application metadata section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
	/// Application name
	pub name: String,
	/// Application version
	pub version: Option<String>,
	/// Application description
	pub description: Option<String>,
}

/// Build configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
	/// Path to the Dockerfile
	pub dockerfile: Option<String>,
	/// Build context directory
	pub context: Option<String>,
}

/// Deployment configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployConfig {
	/// Docker image to deploy
	pub image: Option<String>,
	/// Number of desired replicas
	pub replicas: Option<i32>,
	/// Database backend type (e.g., "PostgreSQL", "MySQL")
	pub database: Option<String>,
}

/// Autoscaling configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScaleConfig {
	/// Minimum number of replicas
	pub min_replicas: Option<i32>,
	/// Maximum number of replicas
	pub max_replicas: Option<i32>,
	/// Metric to scale on (e.g., "Cpu", "Memory", "Rps")
	pub metric: Option<String>,
	/// Target value for the scaling metric
	pub target_value: Option<i32>,
}

impl ScaleConfig {
	/// Validates the autoscaling configuration.
	///
	/// Checks that replica counts are non-negative, max >= min when both
	/// are present, and target_value is positive.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(min) = self.min_replicas
			&& min < 0
		{
			errors.push(ValidationError::new("scale.min_replicas must be >= 0"));
		}

		if let Some(max) = self.max_replicas
			&& max < 0
		{
			errors.push(ValidationError::new("scale.max_replicas must be >= 0"));
		}

		if let (Some(min), Some(max)) = (self.min_replicas, self.max_replicas)
			&& max < min
		{
			errors.push(ValidationError::new(
				"scale.max_replicas must be >= scale.min_replicas",
			));
		}

		if let Some(target) = self.target_value
			&& target <= 0
		{
			errors.push(ValidationError::new("scale.target_value must be > 0"));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

/// Service exposure configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesConfig {
	/// Service port
	pub port: Option<i32>,
	/// Target port on the container
	pub target_port: Option<i32>,
	/// Ingress hostname for external access
	pub ingress_host: Option<String>,
}

impl ServicesConfig {
	/// Validates the service exposure configuration.
	///
	/// Checks that port and target_port are within the valid range (1-65535).
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(port) = self.port
			&& !(1..=65535).contains(&port)
		{
			errors.push(ValidationError::new(
				"services.port must be between 1 and 65535",
			));
		}

		if let Some(target_port) = self.target_port
			&& !(1..=65535).contains(&target_port)
		{
			errors.push(ValidationError::new(
				"services.target_port must be between 1 and 65535",
			));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

/// Health check configuration section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
	/// HTTP path for health checks
	pub path: Option<String>,
	/// Port for health checks
	pub port: Option<i32>,
	/// Interval between health checks in seconds
	pub interval_seconds: Option<i32>,
}

impl HealthConfig {
	/// Validates the health check configuration.
	///
	/// Checks that port is within the valid range (1-65535) and
	/// interval_seconds is positive.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(port) = self.port
			&& !(1..=65535).contains(&port)
		{
			errors.push(ValidationError::new(
				"health.port must be between 1 and 65535",
			));
		}

		if let Some(interval) = self.interval_seconds
			&& interval <= 0
		{
			errors.push(ValidationError::new("health.interval_seconds must be > 0"));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

impl DeployConfig {
	/// Validates the deployment configuration.
	///
	/// Checks that replicas is non-negative.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(replicas) = self.replicas
			&& replicas < 0
		{
			errors.push(ValidationError::new("deploy.replicas must be >= 0"));
		}

		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

impl ReinhardtConfig {
	/// Validates the full configuration.
	///
	/// Delegates to nested config validations and collects all errors.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if let Some(ref deploy) = self.deploy
			&& let Err(errs) = deploy.validate()
		{
			errors.extend(errs);
		}

		if let Some(ref scale) = self.scale
			&& let Err(errs) = scale.validate()
		{
			errors.extend(errs);
		}

		if let Some(ref services) = self.services
			&& let Err(errs) = services.validate()
		{
			errors.extend(errs);
		}

		if let Some(ref health) = self.health
			&& let Err(errs) = health.validate()
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
	fn parse_full_config() {
		// Arrange
		let toml_str = r#"
[app]
name = "myapp"
version = "1.0.0"
description = "My application"

[build]
dockerfile = "Dockerfile"
context = "."

[deploy]
image = "myapp:latest"
replicas = 3
database = "PostgreSQL"

[scale]
min_replicas = 1
max_replicas = 10
metric = "Cpu"
target_value = 80

[services]
port = 80
target_port = 8080
ingress_host = "myapp.example.com"

[health]
path = "/healthz"
port = 8080
interval_seconds = 30
"#;

		// Act
		let config: ReinhardtConfig = toml::from_str(toml_str).expect("parsing should succeed");

		// Assert
		assert_eq!(config.app.name, "myapp");
		assert_eq!(config.app.version.as_deref(), Some("1.0.0"));
		assert_eq!(config.deploy.as_ref().unwrap().replicas, Some(3));
		assert_eq!(config.scale.as_ref().unwrap().max_replicas, Some(10));
		assert_eq!(
			config.services.as_ref().unwrap().ingress_host.as_deref(),
			Some("myapp.example.com")
		);
		assert_eq!(
			config.health.as_ref().unwrap().path.as_deref(),
			Some("/healthz")
		);
	}

	#[rstest]
	fn parse_minimal_config() {
		// Arrange
		let toml_str = r#"
[app]
name = "minimal-app"
"#;

		// Act
		let config: ReinhardtConfig = toml::from_str(toml_str).expect("parsing should succeed");

		// Assert
		assert_eq!(config.app.name, "minimal-app");
		assert_eq!(config.app.version, None);
		assert_eq!(config.app.description, None);
		assert!(config.build.is_none());
		assert!(config.deploy.is_none());
		assert!(config.scale.is_none());
		assert!(config.services.is_none());
		assert!(config.health.is_none());
	}

	#[rstest]
	fn scale_config_validation_valid() {
		// Arrange
		let config = ScaleConfig {
			min_replicas: Some(1),
			max_replicas: Some(10),
			metric: Some("Cpu".to_string()),
			target_value: Some(80),
		};

		// Act
		let result = config.validate();

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn scale_config_validation_invalid() {
		// Arrange
		let config = ScaleConfig {
			min_replicas: Some(-1),
			max_replicas: Some(-2),
			metric: None,
			target_value: Some(0),
		};

		// Act
		let result = config.validate();

		// Assert
		let errors = result.unwrap_err();
		// min(-1) + max(-2) + max<min + target(0)
		assert_eq!(errors.len(), 4);
		assert_eq!(errors[0].message, "scale.min_replicas must be >= 0");
		assert_eq!(errors[1].message, "scale.max_replicas must be >= 0");
		assert_eq!(
			errors[2].message,
			"scale.max_replicas must be >= scale.min_replicas"
		);
		assert_eq!(errors[3].message, "scale.target_value must be > 0");
	}

	#[rstest]
	fn services_config_validation_invalid_port() {
		// Arrange
		let config = ServicesConfig {
			port: Some(0),
			target_port: Some(65536),
			ingress_host: None,
		};

		// Act
		let result = config.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 2);
		assert_eq!(
			errors[0].message,
			"services.port must be between 1 and 65535"
		);
		assert_eq!(
			errors[1].message,
			"services.target_port must be between 1 and 65535"
		);
	}

	#[rstest]
	fn health_config_validation_invalid() {
		// Arrange
		let config = HealthConfig {
			path: None,
			port: Some(0),
			interval_seconds: Some(0),
		};

		// Act
		let result = config.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 2);
		assert_eq!(errors[0].message, "health.port must be between 1 and 65535");
		assert_eq!(errors[1].message, "health.interval_seconds must be > 0");
	}

	#[rstest]
	fn reinhardt_config_validation_collects_errors() {
		// Arrange
		let config = ReinhardtConfig {
			app: AppConfig {
				name: "test-app".to_string(),
				version: None,
				description: None,
			},
			build: None,
			deploy: Some(DeployConfig {
				image: None,
				replicas: Some(-1),
				database: None,
			}),
			scale: Some(ScaleConfig {
				min_replicas: Some(-1),
				max_replicas: None,
				metric: None,
				target_value: None,
			}),
			services: Some(ServicesConfig {
				port: Some(0),
				target_port: None,
				ingress_host: None,
			}),
			health: Some(HealthConfig {
				path: None,
				port: None,
				interval_seconds: Some(0),
			}),
		};

		// Act
		let result = config.validate();

		// Assert
		let errors = result.unwrap_err();
		// deploy.replicas(-1) + scale.min(-1) + services.port(0) + health.interval(0)
		assert_eq!(errors.len(), 4);
		assert_eq!(errors[0].message, "deploy.replicas must be >= 0");
		assert_eq!(errors[1].message, "scale.min_replicas must be >= 0");
		assert_eq!(
			errors[2].message,
			"services.port must be between 1 and 65535"
		);
		assert_eq!(errors[3].message, "health.interval_seconds must be > 0");
	}

	#[rstest]
	fn optional_fields_default_to_none() {
		// Arrange
		let toml_str = r#"
[app]
name = "test-app"

[deploy]
image = "test:v1"
"#;

		// Act
		let config: ReinhardtConfig = toml::from_str(toml_str).expect("parsing should succeed");

		// Assert
		let deploy = config.deploy.expect("deploy section should be present");
		assert_eq!(deploy.image.as_deref(), Some("test:v1"));
		assert_eq!(deploy.replicas, None);
		assert_eq!(deploy.database, None);
	}
}
