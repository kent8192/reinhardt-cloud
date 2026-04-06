//! Plugin registry for discovering and managing plugins.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tracing::{info, warn};

use super::traits::{PluginHookType, PluginResult, PluginService};
use crate::error::ApiError;

/// Configuration for a registered plugin.
#[derive(Debug, Clone)]
pub struct PluginConfig {
	/// Plugin name.
	pub name: String,
	/// gRPC endpoint (e.g. "http://localhost:50052").
	pub endpoint: String,
	/// Hook type this plugin handles.
	pub hook_type: PluginHookType,
	/// Execution timeout.
	pub timeout: Duration,
}

/// Registry managing all registered plugins.
///
/// Plugins are organized by hook type for pipeline execution.
pub struct PluginRegistry {
	plugins: HashMap<String, Arc<dyn PluginService>>,
	hooks: HashMap<PluginHookType, Vec<String>>,
}

impl PluginRegistry {
	pub fn new() -> Self {
		Self {
			plugins: HashMap::new(),
			hooks: HashMap::new(),
		}
	}

	/// Register a plugin service.
	pub fn register(&mut self, plugin: Arc<dyn PluginService>) {
		let name = plugin.name().to_string();
		for hook_type in plugin.hook_types() {
			self.hooks
				.entry(hook_type.clone())
				.or_default()
				.push(name.clone());
		}
		info!(plugin = %name, "Plugin registered");
		self.plugins.insert(name, plugin);
	}

	/// Get all plugins for a given hook type.
	pub fn plugins_for_hook(&self, hook_type: &PluginHookType) -> Vec<Arc<dyn PluginService>> {
		self.hooks
			.get(hook_type)
			.map(|names| {
				names
					.iter()
					.filter_map(|name| self.plugins.get(name).cloned())
					.collect()
			})
			.unwrap_or_default()
	}

	/// Execute all plugins for a given hook type in sequence.
	///
	/// Returns the results from each plugin. Stops on fatal errors.
	pub async fn execute_hook(
		&self,
		hook_type: &PluginHookType,
		input: &[u8],
		context: HashMap<String, String>,
	) -> Result<Vec<PluginResult>, ApiError> {
		let plugins = self.plugins_for_hook(hook_type);
		let mut results = Vec::with_capacity(plugins.len());

		for plugin in &plugins {
			let result = plugin
				.run_function(&format!("{hook_type:?}"), input, context.clone())
				.await?;

			if !result.success {
				warn!(
					plugin = plugin.name(),
					hook = ?hook_type,
					"Plugin execution failed"
				);
			}

			let is_fatal = result
				.conditions
				.iter()
				.any(|c| c.severity == super::traits::ConditionSeverity::Error);

			results.push(result);

			if is_fatal {
				return Err(ApiError::Internal(format!(
					"Plugin {} returned fatal error",
					plugin.name()
				)));
			}
		}

		Ok(results)
	}

	/// Number of registered plugins.
	pub fn count(&self) -> usize {
		self.plugins.len()
	}

	/// List all registered plugin names.
	pub fn list_plugins(&self) -> Vec<String> {
		self.plugins.keys().cloned().collect()
	}

	/// Run health checks on all plugins.
	pub async fn health_check_all(&self) -> HashMap<String, bool> {
		let mut results = HashMap::new();
		for (name, plugin) in &self.plugins {
			results.insert(name.clone(), plugin.health_check().await);
		}
		results
	}
}

impl Default for PluginRegistry {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	struct TestPlugin {
		name: String,
		hooks: Vec<PluginHookType>,
	}

	#[async_trait::async_trait]
	impl PluginService for TestPlugin {
		async fn run_function(
			&self,
			_function_name: &str,
			_input: &[u8],
			_context: HashMap<String, String>,
		) -> Result<PluginResult, ApiError> {
			Ok(PluginResult {
				success: true,
				output: b"ok".to_vec(),
				conditions: vec![],
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
	fn test_register_and_list() {
		// Arrange
		let mut registry = PluginRegistry::new();
		let plugin = Arc::new(TestPlugin {
			name: "scanner".to_string(),
			hooks: vec![PluginHookType::PreBuild],
		});

		// Act
		registry.register(plugin);

		// Assert
		assert_eq!(registry.count(), 1);
		assert!(registry.list_plugins().contains(&"scanner".to_string()));
	}

	#[rstest]
	fn test_plugins_for_hook() {
		// Arrange
		let mut registry = PluginRegistry::new();
		registry.register(Arc::new(TestPlugin {
			name: "pre-build-1".to_string(),
			hooks: vec![PluginHookType::PreBuild],
		}));
		registry.register(Arc::new(TestPlugin {
			name: "post-build-1".to_string(),
			hooks: vec![PluginHookType::PostBuild],
		}));

		// Act
		let pre_build = registry.plugins_for_hook(&PluginHookType::PreBuild);
		let post_deploy = registry.plugins_for_hook(&PluginHookType::PostDeploy);

		// Assert
		assert_eq!(pre_build.len(), 1);
		assert_eq!(post_deploy.len(), 0);
	}

	#[rstest]
	#[tokio::test]
	async fn test_execute_hook() {
		// Arrange
		let mut registry = PluginRegistry::new();
		registry.register(Arc::new(TestPlugin {
			name: "validator".to_string(),
			hooks: vec![PluginHookType::PreDeploy],
		}));

		// Act
		let results = registry
			.execute_hook(&PluginHookType::PreDeploy, b"{}", HashMap::new())
			.await
			.unwrap();

		// Assert
		assert_eq!(results.len(), 1);
		assert!(results[0].success);
	}

	#[rstest]
	#[tokio::test]
	async fn test_health_check_all() {
		// Arrange
		let mut registry = PluginRegistry::new();
		registry.register(Arc::new(TestPlugin {
			name: "healthy-plugin".to_string(),
			hooks: vec![PluginHookType::PreBuild],
		}));

		// Act
		let health = registry.health_check_all().await;

		// Assert
		assert_eq!(health.get("healthy-plugin"), Some(&true));
	}
}
