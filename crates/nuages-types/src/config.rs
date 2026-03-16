//! Configuration schema types for `reinhardt.toml`.
//!
//! Defines the structure of the `reinhardt.toml` configuration file
//! used to declare application deployment settings.

use serde::{Deserialize, Serialize};

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
