//! Integration tests for the plugin pipeline system.
//!
//! These tests verify that `PluginRegistry` correctly orchestrates execution
//! of multiple plugins, handles fatal error conditions, and preserves
//! execution ordering.

use std::collections::HashMap;
use std::sync::Arc;

use reinhardt_cloud_core::error::ApiError;
use reinhardt_cloud_core::plugin::registry::PluginRegistry;
use reinhardt_cloud_core::plugin::traits::{
	ConditionSeverity, PluginCondition, PluginHookType, PluginResult, PluginService,
};
use rstest::rstest;

/// A configurable test plugin for integration testing.
struct TestPlugin {
	name: String,
	hooks: Vec<PluginHookType>,
	success: bool,
	fatal: bool,
}

#[async_trait::async_trait]
impl PluginService for TestPlugin {
	async fn run_function(
		&self,
		_function_name: &str,
		_input: &[u8],
		_context: HashMap<String, String>,
	) -> Result<PluginResult, ApiError> {
		let conditions = if self.fatal {
			vec![PluginCondition {
				condition_type: "error".to_string(),
				message: format!("Fatal error from plugin '{}'", self.name),
				severity: ConditionSeverity::Error,
			}]
		} else {
			vec![]
		};

		Ok(PluginResult {
			success: self.success,
			output: format!("output-from-{}", self.name).into_bytes(),
			conditions,
		})
	}

	fn name(&self) -> &str {
		&self.name
	}

	fn hook_types(&self) -> &[PluginHookType] {
		&self.hooks
	}

	async fn health_check(&self) -> bool {
		true
	}
}

#[rstest]
#[tokio::test]
async fn test_plugin_pre_build_hook_execution() {
	// Arrange -- register 2 plugins for PreBuild
	let mut registry = PluginRegistry::new();
	registry.register(Arc::new(TestPlugin {
		name: "scanner-plugin".to_string(),
		hooks: vec![PluginHookType::PreBuild],
		success: true,
		fatal: false,
	}));
	registry.register(Arc::new(TestPlugin {
		name: "linter-plugin".to_string(),
		hooks: vec![PluginHookType::PreBuild],
		success: true,
		fatal: false,
	}));

	// Act -- execute the PreBuild hook
	let results = registry
		.execute_hook(&PluginHookType::PreBuild, b"{}", HashMap::new())
		.await
		.unwrap();

	// Assert -- both plugins should have been executed
	assert_eq!(results.len(), 2, "Expected 2 results from 2 plugins");

	assert!(results[0].success, "First plugin should succeed");
	assert_eq!(
		results[0].conditions.len(),
		0,
		"First plugin should have no conditions"
	);
	assert_eq!(
		String::from_utf8_lossy(&results[0].output),
		"output-from-scanner-plugin",
		"First plugin output mismatch"
	);

	assert!(results[1].success, "Second plugin should succeed");
	assert_eq!(
		results[1].conditions.len(),
		0,
		"Second plugin should have no conditions"
	);
	assert_eq!(
		String::from_utf8_lossy(&results[1].output),
		"output-from-linter-plugin",
		"Second plugin output mismatch"
	);
}

#[rstest]
#[tokio::test]
async fn test_plugin_fatal_error_stops_pipeline() {
	// Arrange -- register a plugin that returns a fatal condition (Error severity)
	let mut registry = PluginRegistry::new();
	registry.register(Arc::new(TestPlugin {
		name: "fatal-plugin".to_string(),
		hooks: vec![PluginHookType::PreBuild],
		success: false,
		fatal: true,
	}));
	// This plugin should never execute because the first one is fatal
	registry.register(Arc::new(TestPlugin {
		name: "unreachable-plugin".to_string(),
		hooks: vec![PluginHookType::PreBuild],
		success: true,
		fatal: false,
	}));

	// Act -- execute the PreBuild hook
	let result = registry
		.execute_hook(&PluginHookType::PreBuild, b"{}", HashMap::new())
		.await;

	// Assert -- should return Err because the first plugin had a fatal condition
	assert!(
		result.is_err(),
		"Expected execute_hook to return Err for fatal plugin"
	);
	let err = result.unwrap_err();
	let err_msg = err.to_string();
	assert!(
		err_msg.contains("fatal-plugin"),
		"Error should mention the failing plugin name, got: {err_msg}"
	);
}

#[rstest]
#[tokio::test]
async fn test_multiple_plugins_execute_in_order() {
	// Arrange -- register 3 plugins for the same hook
	let mut registry = PluginRegistry::new();
	registry.register(Arc::new(TestPlugin {
		name: "first".to_string(),
		hooks: vec![PluginHookType::PostBuild],
		success: true,
		fatal: false,
	}));
	registry.register(Arc::new(TestPlugin {
		name: "second".to_string(),
		hooks: vec![PluginHookType::PostBuild],
		success: true,
		fatal: false,
	}));
	registry.register(Arc::new(TestPlugin {
		name: "third".to_string(),
		hooks: vec![PluginHookType::PostBuild],
		success: true,
		fatal: false,
	}));

	// Act -- execute the PostBuild hook
	let results = registry
		.execute_hook(&PluginHookType::PostBuild, b"test-input", HashMap::new())
		.await
		.unwrap();

	// Assert -- all 3 plugins executed and results are in registration order
	assert_eq!(results.len(), 3, "Expected 3 results from 3 plugins");
	assert_eq!(
		String::from_utf8_lossy(&results[0].output),
		"output-from-first",
		"First result should be from 'first' plugin"
	);
	assert_eq!(
		String::from_utf8_lossy(&results[1].output),
		"output-from-second",
		"Second result should be from 'second' plugin"
	);
	assert_eq!(
		String::from_utf8_lossy(&results[2].output),
		"output-from-third",
		"Third result should be from 'third' plugin"
	);

	// Verify all succeeded
	for (i, result) in results.iter().enumerate() {
		assert!(result.success, "Plugin {i} should succeed");
		assert!(
			result.conditions.is_empty(),
			"Plugin {i} should have no conditions"
		);
	}
}
