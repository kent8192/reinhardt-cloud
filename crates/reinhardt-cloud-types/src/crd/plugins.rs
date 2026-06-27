//! Plugin specification types for the `Project` custom resource.
//!
//! Enables declarative attachment of dentdelion WASM plugins to an
//! application. The operator materializes each `PluginSpec` into a
//! `dentdelion.toml` `ConfigMap` and mounts the WASM artifacts into
//! the application container via volume + volume mount pairs.

use std::path::{Component, Path};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::validation::ValidationError;

/// Safe in-container root for writable dentdelion WASM plugin mounts.
pub const PLUGIN_WASM_DIR_PREFIX: &str = "/var/lib/dentdelion";

/// Sanitizes a plugin name into the suffix used by the Kubernetes
/// `Volume.name` (excluding any operator-side prefix).
///
/// The operator pairs this with a fixed prefix (`dentdelion-…`) when
/// materializing volumes. Two distinct plugin names that produce the
/// same sanitized suffix would collide on the resulting `Volume.name`
/// and Kubernetes would reject the PodSpec at admission, so this
/// helper is also used by [`crate::crd::ProjectSpec::validate`]
/// to detect such collisions before they reach the cluster.
pub fn sanitized_volume_suffix(name: &str) -> String {
	let sanitized: String = name
		.chars()
		.map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
		.collect::<String>()
		.to_ascii_lowercase();
	let trimmed = sanitized.trim_matches('-').to_string();
	if trimmed.is_empty() {
		"plugin".to_string()
	} else {
		trimmed
	}
}

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
	/// Logical name of the plugin, used as an identifier in the rendered
	/// `dentdelion.toml` and as the basis for the per-plugin Kubernetes
	/// `Volume.name` (sanitized via [`sanitized_volume_suffix`]).
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
	/// Checks that `name` and `wasm_dir` are non-empty, that `wasm_dir`
	/// is an absolute path under [`PLUGIN_WASM_DIR_PREFIX`] with no `..`
	/// components (path-traversal guard, since `wasm_dir` flows directly
	/// into a Kubernetes `VolumeMount.mount_path`), and that any optional
	/// numeric limits are strictly positive when provided.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();

		if self.name.trim().is_empty() {
			errors.push(ValidationError::new("plugins[].name must be non-empty"));
		}

		let trimmed_dir = self.wasm_dir.trim();
		if trimmed_dir.is_empty() {
			errors.push(ValidationError::new("plugins[].wasm_dir must be non-empty"));
		} else {
			let wasm_path = Path::new(trimmed_dir);
			let is_absolute = wasm_path.is_absolute();
			if !is_absolute {
				errors.push(ValidationError::new(
					"plugins[].wasm_dir must be an absolute path (start with '/')",
				));
			}
			let has_parent_dir = wasm_path
				.components()
				.any(|c| matches!(c, Component::ParentDir));
			if has_parent_dir {
				errors.push(ValidationError::new(
					"plugins[].wasm_dir must not contain '..' path components",
				));
			}
			if is_absolute
				&& !has_parent_dir
				&& (!wasm_path.starts_with(PLUGIN_WASM_DIR_PREFIX)
					|| wasm_path == Path::new(PLUGIN_WASM_DIR_PREFIX))
			{
				errors.push(ValidationError::new(format!(
					"plugins[].wasm_dir must be under {PLUGIN_WASM_DIR_PREFIX}/"
				)));
			}
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
		let json =
			r#"{"name":"p","wasm_dir":"/var/lib/dentdelion/p","plugin_type":"GrpcInterceptor"}"#;

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
			wasm_dir: "/var/lib/dentdelion/p".to_string(),
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

	#[rstest]
	fn plugin_spec_validate_rejects_relative_wasm_dir() {
		// Arrange
		let spec = PluginSpec {
			name: "p".to_string(),
			wasm_dir: "var/lib/dentdelion/p".to_string(),
			plugin_type: PluginType::HttpMiddleware,
			memory_limit_mb: None,
			timeout_ms: None,
			capabilities: Vec::new(),
		};

		// Act
		let errors = spec
			.validate()
			.expect_err("relative wasm_dir should be rejected");

		// Assert
		assert_eq!(errors.len(), 1);
		assert_eq!(
			errors[0].message,
			"plugins[].wasm_dir must be an absolute path (start with '/')"
		);
	}

	#[rstest]
	fn plugin_spec_validate_rejects_path_traversal_in_wasm_dir() {
		// Arrange
		let spec = PluginSpec {
			name: "p".to_string(),
			wasm_dir: "/var/lib/../etc/passwd".to_string(),
			plugin_type: PluginType::HttpMiddleware,
			memory_limit_mb: None,
			timeout_ms: None,
			capabilities: Vec::new(),
		};

		// Act
		let errors = spec
			.validate()
			.expect_err("traversal in wasm_dir should be rejected");

		// Assert
		assert_eq!(errors.len(), 1);
		assert_eq!(
			errors[0].message,
			"plugins[].wasm_dir must not contain '..' path components"
		);
	}

	#[rstest]
	fn plugin_spec_validate_rejects_unsafe_wasm_dir_prefix() {
		// Arrange
		let spec = PluginSpec {
			name: "p".to_string(),
			wasm_dir: "/app".to_string(),
			plugin_type: PluginType::HttpMiddleware,
			memory_limit_mb: None,
			timeout_ms: None,
			capabilities: Vec::new(),
		};

		// Act
		let errors = spec
			.validate()
			.expect_err("unsafe wasm_dir prefix should be rejected");

		// Assert
		assert_eq!(errors.len(), 1);
		assert_eq!(
			errors[0].message,
			"plugins[].wasm_dir must be under /var/lib/dentdelion/"
		);
	}

	#[rstest]
	#[case::dot_versus_dash("my.plugin", "my-plugin", "my-plugin")]
	#[case::underscore_versus_dash("my_plugin", "my-plugin", "my-plugin")]
	#[case::case_versus_lower("MyPlugin", "myplugin", "myplugin")]
	fn sanitized_volume_suffix_collides_for_equivalent_names(
		#[case] a: &str,
		#[case] b: &str,
		#[case] expected: &str,
	) {
		// Arrange & Act
		let suffix_a = sanitized_volume_suffix(a);
		let suffix_b = sanitized_volume_suffix(b);

		// Assert
		assert_eq!(suffix_a, expected);
		assert_eq!(suffix_b, expected);
	}

	#[rstest]
	fn sanitized_volume_suffix_falls_back_when_only_separators() {
		// Arrange & Act
		let suffix = sanitized_volume_suffix("---");

		// Assert
		assert_eq!(suffix, "plugin");
	}
}
