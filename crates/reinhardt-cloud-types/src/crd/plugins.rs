//! Plugin specification types for the `ReinhardtApp` custom resource.
//!
//! Enables declarative attachment of dentdelion WASM plugins to an
//! application. The operator materializes each `PluginSpec` into a
//! `dentdelion.toml` `ConfigMap` and mounts the WASM artifacts into
//! the application container via volume + volume mount pairs.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::validation::ValidationError;

/// Category of a dentdelion WASM plugin.
///
/// Describes the extension point the plugin attaches to.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum PluginType {
	/// HTTP middleware plugin, applied to incoming HTTP requests.
	HttpMiddleware,
	/// gRPC interceptor plugin, applied to gRPC calls.
	GrpcInterceptor,
	/// Event handler plugin, applied to background event streams.
	EventHandler,
}

/// Capability granted to a plugin at runtime.
///
/// Capabilities follow the principle of least privilege: plugins
/// only receive access to the listed categories.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum PluginCapability {
	/// Plugin is permitted to make outbound network calls.
	NetworkAccess,
	/// Plugin is permitted to read from the filesystem.
	FilesystemRead,
	/// Plugin is permitted to write to the filesystem.
	FilesystemWrite,
	/// Plugin is permitted to read environment variables.
	EnvAccess,
}

/// Declarative configuration for a single dentdelion WASM plugin.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct PluginSpec {
	/// Logical name of the plugin, used as an identifier in config and
	/// as the `subPath` for the mounted WASM artifact.
	pub name: String,
	/// Directory inside the pod where the WASM artifact(s) are mounted.
	pub wasm_dir: String,
	/// Extension point the plugin attaches to.
	pub plugin_type: PluginType,
	/// Optional memory limit in megabytes for the WASM sandbox.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub memory_limit_mb: Option<u64>,
	/// Optional per-invocation timeout in milliseconds.
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub timeout_ms: Option<u64>,
	/// Capabilities granted to the plugin at runtime.
	#[serde(default)]
	pub capabilities: Vec<PluginCapability>,
}

impl PluginSpec {
	/// Validates the plugin specification.
	///
	/// Checks that `name` and `wasm_dir` are non-empty, and that any
	/// optional numeric limits are strictly positive when provided.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if self.name.trim().is_empty() {
			errors.push(ValidationError::new("plugins[].name must be non-empty"));
		}

		if self.wasm_dir.trim().is_empty() {
			errors.push(ValidationError::new("plugins[].wasm_dir must be non-empty"));
		}

		if let Some(mem) = self.memory_limit_mb
			&& mem == 0
		{
			errors.push(ValidationError::new(
				"plugins[].memory_limit_mb must be > 0",
			));
		}

		if let Some(timeout) = self.timeout_ms
			&& timeout == 0
		{
			errors.push(ValidationError::new("plugins[].timeout_ms must be > 0"));
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
	fn plugin_spec_roundtrip_json() {
		// Arrange
		let spec = PluginSpec {
			name: "auth-gate".to_string(),
			wasm_dir: "/var/lib/dentdelion/auth-gate".to_string(),
			plugin_type: PluginType::HttpMiddleware,
			memory_limit_mb: Some(64),
			timeout_ms: Some(500),
			capabilities: vec![PluginCapability::NetworkAccess, PluginCapability::EnvAccess],
		};

		// Act
		let json = serde_json::to_string(&spec).expect("serialize should succeed");
		let parsed: PluginSpec = serde_json::from_str(&json).expect("deserialize should succeed");

		// Assert
		assert_eq!(parsed.name, "auth-gate");
		assert_eq!(parsed.plugin_type, PluginType::HttpMiddleware);
		assert_eq!(parsed.memory_limit_mb, Some(64));
		assert_eq!(parsed.timeout_ms, Some(500));
		assert_eq!(parsed.capabilities.len(), 2);
		assert_eq!(parsed.capabilities[0], PluginCapability::NetworkAccess);
	}

	#[rstest]
	fn plugin_spec_capabilities_defaults_empty() {
		// Arrange
		let json = r#"{"name":"p","wasm_dir":"/p","plugin_type":"GrpcInterceptor"}"#;

		// Act
		let parsed: PluginSpec = serde_json::from_str(json).expect("deserialize should succeed");

		// Assert
		assert_eq!(parsed.plugin_type, PluginType::GrpcInterceptor);
		assert!(parsed.capabilities.is_empty());
		assert!(parsed.memory_limit_mb.is_none());
		assert!(parsed.timeout_ms.is_none());
	}

	#[rstest]
	fn plugin_spec_validate_accepts_minimal() {
		// Arrange
		let spec = PluginSpec {
			name: "p".to_string(),
			wasm_dir: "/p".to_string(),
			plugin_type: PluginType::EventHandler,
			memory_limit_mb: None,
			timeout_ms: None,
			capabilities: Vec::new(),
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn plugin_spec_validate_rejects_empty_name_and_dir() {
		// Arrange
		let spec = PluginSpec {
			name: "   ".to_string(),
			wasm_dir: String::new(),
			plugin_type: PluginType::HttpMiddleware,
			memory_limit_mb: Some(0),
			timeout_ms: Some(0),
			capabilities: Vec::new(),
		};

		// Act
		let errors = spec.validate().expect_err("validation should fail");

		// Assert
		assert_eq!(errors.len(), 4);
		assert_eq!(errors[0].message, "plugins[].name must be non-empty");
		assert_eq!(errors[1].message, "plugins[].wasm_dir must be non-empty");
		assert_eq!(errors[2].message, "plugins[].memory_limit_mb must be > 0");
		assert_eq!(errors[3].message, "plugins[].timeout_ms must be > 0");
	}
}
