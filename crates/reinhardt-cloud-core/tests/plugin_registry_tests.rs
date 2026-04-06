//! Integration tests for PluginRegistry.

mod fixtures;

use std::collections::HashMap;

use rstest::rstest;

use reinhardt_cloud_core::error::ApiError;
use reinhardt_cloud_core::plugin::registry::PluginRegistry;
use reinhardt_cloud_core::plugin::traits::PluginHookType;

use fixtures::{plugin_registry, test_plugin_fatal, test_plugin_success};

// ===========================================================================
// Happy path tests
// ===========================================================================

#[rstest]
fn test_plugin_multi_hook_registration(mut plugin_registry: PluginRegistry) {
	// Arrange
	let plugin = test_plugin_success(
		"multi-hook",
		vec![PluginHookType::PreBuild, PluginHookType::PostBuild],
	);

	// Act
	plugin_registry.register(plugin);

	// Assert — plugin is found for both hooks, count is 1 (single plugin)
	let pre_build = plugin_registry.plugins_for_hook(&PluginHookType::PreBuild);
	let post_build = plugin_registry.plugins_for_hook(&PluginHookType::PostBuild);
	assert_eq!(pre_build.len(), 1);
	assert_eq!(post_build.len(), 1);
	assert_eq!(plugin_registry.count(), 1);
}

// ===========================================================================
// Error path tests
// ===========================================================================

#[rstest]
#[tokio::test]
async fn test_execute_hook_fatal_error_short_circuits(mut plugin_registry: PluginRegistry) {
	// Arrange — register a fatal plugin first, then a success plugin
	let fatal_plugin = test_plugin_fatal("fatal-first", vec![PluginHookType::PreBuild]);
	let success_plugin = test_plugin_success("success-second", vec![PluginHookType::PreBuild]);
	plugin_registry.register(fatal_plugin);
	plugin_registry.register(success_plugin);

	// Act
	let result = plugin_registry
		.execute_hook(&PluginHookType::PreBuild, b"{}", HashMap::new())
		.await;

	// Assert — should fail with Internal error from the fatal plugin
	match result {
		Err(ApiError::Internal(msg)) => {
			assert!(
				msg.contains("fatal-first"),
				"Error should mention the fatal plugin name, got: {msg}"
			);
		}
		other => panic!("Expected Err(ApiError::Internal), got: {other:?}"),
	}
}

// ===========================================================================
// Edge case tests
// ===========================================================================

#[rstest]
#[tokio::test]
async fn test_plugin_execute_no_registered_plugins(plugin_registry: PluginRegistry) {
	// Arrange — empty registry

	// Act
	let result = plugin_registry
		.execute_hook(&PluginHookType::PreDeploy, b"{}", HashMap::new())
		.await
		.unwrap();

	// Assert
	assert!(result.is_empty(), "No plugins -> empty results vec");
}

// ===========================================================================
// State transition tests
// ===========================================================================

#[rstest]
#[tokio::test]
async fn test_plugin_register_then_execute_then_health_check(mut plugin_registry: PluginRegistry) {
	// Arrange
	let plugin = test_plugin_success("lifecycle", vec![PluginHookType::PreBuild]);
	plugin_registry.register(plugin);

	// Act — execute hook
	let exec_results = plugin_registry
		.execute_hook(&PluginHookType::PreBuild, b"{}", HashMap::new())
		.await
		.unwrap();

	// Act — health check
	let health = plugin_registry.health_check_all().await;

	// Assert — both succeed
	assert_eq!(exec_results.len(), 1);
	assert!(exec_results[0].success);
	assert_eq!(health.get("lifecycle"), Some(&true));
}

// ===========================================================================
// Use case tests
// ===========================================================================

#[rstest]
#[tokio::test]
async fn test_usecase_prebuild_pipeline_3_plugins(mut plugin_registry: PluginRegistry) {
	// Arrange
	for i in 0..3 {
		let plugin = test_plugin_success(
			&format!("prebuild-plugin-{i}"),
			vec![PluginHookType::PreBuild],
		);
		plugin_registry.register(plugin);
	}

	// Act
	let results = plugin_registry
		.execute_hook(&PluginHookType::PreBuild, b"{}", HashMap::new())
		.await
		.unwrap();

	// Assert — all 3 return success
	assert_eq!(results.len(), 3);
	for result in &results {
		assert!(result.success);
	}
}

// ===========================================================================
// Combination tests
// ===========================================================================

#[rstest]
fn test_multiple_hooks_multiple_plugins(mut plugin_registry: PluginRegistry) {
	// Arrange
	plugin_registry.register(test_plugin_success("pre-a", vec![PluginHookType::PreBuild]));
	plugin_registry.register(test_plugin_success("pre-b", vec![PluginHookType::PreBuild]));
	plugin_registry.register(test_plugin_success(
		"post-a",
		vec![PluginHookType::PostBuild],
	));
	plugin_registry.register(test_plugin_success(
		"post-b",
		vec![PluginHookType::PostBuild],
	));

	// Act
	let pre = plugin_registry.plugins_for_hook(&PluginHookType::PreBuild);
	let post = plugin_registry.plugins_for_hook(&PluginHookType::PostBuild);
	let deploy = plugin_registry.plugins_for_hook(&PluginHookType::PreDeploy);

	// Assert
	assert_eq!(pre.len(), 2);
	assert_eq!(post.len(), 2);
	assert_eq!(deploy.len(), 0);
	assert_eq!(plugin_registry.count(), 4);
}

// ===========================================================================
// Decision table tests
// ===========================================================================

#[rstest]
#[case(true, false, true)] // Plugin succeeds, no fatal -> Ok
#[case(false, false, true)] // Plugin fails but no fatal condition -> Ok (result collected)
#[case(false, true, false)] // Plugin fails with fatal condition -> Err
#[tokio::test]
async fn test_plugin_execute_result_decision_table(
	#[case] plugin_success: bool,
	#[case] has_fatal_condition: bool,
	#[case] expect_ok: bool,
) {
	// Arrange
	let mut registry = PluginRegistry::new();

	if has_fatal_condition {
		registry.register(test_plugin_fatal(
			"decision-plugin",
			vec![PluginHookType::PreBuild],
		));
	} else if plugin_success {
		registry.register(test_plugin_success(
			"decision-plugin",
			vec![PluginHookType::PreBuild],
		));
	} else {
		// Non-fatal failure: plugin returns success=false but no fatal condition
		use std::sync::Arc;
		registry.register(Arc::new(fixtures::TestPlugin::new(
			"decision-plugin",
			vec![PluginHookType::PreBuild],
			false,
			false,
		)));
	}

	// Act
	let result = registry
		.execute_hook(&PluginHookType::PreBuild, b"{}", HashMap::new())
		.await;

	// Assert
	assert_eq!(
		result.is_ok(),
		expect_ok,
		"plugin_success={plugin_success}, has_fatal={has_fatal_condition}"
	);
}
