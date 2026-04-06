//! Plugin service trait definitions.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ApiError;

/// The type of pipeline hook a plugin handles.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginHookType {
	/// Runs before a build starts.
	PreBuild,
	/// Runs after a build completes.
	PostBuild,
	/// Runs before a deployment.
	PreDeploy,
	/// Runs after a deployment.
	PostDeploy,
}

/// Result of a plugin function execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResult {
	/// Whether the function succeeded.
	pub success: bool,
	/// Output data (JSON bytes).
	pub output: Vec<u8>,
	/// Conditions/warnings from the plugin.
	pub conditions: Vec<PluginCondition>,
}

/// A condition reported by a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCondition {
	pub condition_type: String,
	pub message: String,
	pub severity: ConditionSeverity,
}

/// Severity of a plugin condition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConditionSeverity {
	Info,
	Warning,
	Error,
}

/// Trait for plugin function execution.
///
/// Implementations may be local (in-process) or remote (gRPC client).
#[async_trait]
pub trait PluginService: Send + Sync + 'static {
	/// Execute a plugin function synchronously.
	async fn run_function(
		&self,
		function_name: &str,
		input: &[u8],
		context: HashMap<String, String>,
	) -> Result<PluginResult, ApiError>;

	/// Get the name of this plugin.
	fn name(&self) -> &str;

	/// Get the hook types this plugin handles.
	fn hook_types(&self) -> &[PluginHookType];

	/// Health check — returns true if the plugin is responsive.
	async fn health_check(&self) -> bool;
}
