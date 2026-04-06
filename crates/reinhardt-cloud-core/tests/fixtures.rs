//! Shared test fixtures for reinhardt-cloud-core integration tests.
//!
//! Provides composable rstest fixtures for build services, log services,
//! plugin registries, and helper constructors for domain types.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use rstest::fixture;
use uuid::Uuid;

use reinhardt_cloud_core::error::ApiError;
use reinhardt_cloud_core::plugin::registry::PluginRegistry;
use reinhardt_cloud_core::plugin::traits::{
    ConditionSeverity, PluginCondition, PluginHookType, PluginResult, PluginService,
};
use reinhardt_cloud_core::services::build::local::LocalBuildService;
use reinhardt_cloud_core::services::log::buffer::LogBuffer;
use reinhardt_cloud_core::services::log::local::LocalLogService;
use reinhardt_cloud_types::build::BuildRequest;
use reinhardt_cloud_types::log::{LogEntry, LogLevel};

// ---------------------------------------------------------------------------
// Build fixtures
// ---------------------------------------------------------------------------

/// Creates a fresh `LocalBuildService` instance.
#[fixture]
pub fn local_build_service() -> LocalBuildService {
    LocalBuildService::new()
}

/// Creates a `BuildRequest` with a unique `app_name` to avoid collisions.
#[fixture]
pub fn build_request() -> BuildRequest {
    BuildRequest {
        app_name: format!("test-app-{}", Uuid::new_v4()),
        image: "registry.example.com/test:latest".to_string(),
        env_vars: vec![],
        dockerfile: None,
        context_path: None,
    }
}

// ---------------------------------------------------------------------------
// Log fixtures
// ---------------------------------------------------------------------------

/// Creates a shared `LogBuffer` with capacity 100.
#[fixture]
pub fn log_buffer() -> Arc<LogBuffer> {
    Arc::new(LogBuffer::new(100))
}

/// Creates a shared `LogBuffer` with capacity 3 (for overflow tests).
#[fixture]
pub fn log_buffer_small() -> Arc<LogBuffer> {
    Arc::new(LogBuffer::new(3))
}

/// Creates a `LocalLogService` backed by the provided `LogBuffer`.
#[fixture]
pub fn local_log_service(log_buffer: Arc<LogBuffer>) -> LocalLogService {
    LocalLogService::new(log_buffer)
}

// ---------------------------------------------------------------------------
// Plugin fixtures
// ---------------------------------------------------------------------------

/// Creates an empty `PluginRegistry`.
#[fixture]
pub fn plugin_registry() -> PluginRegistry {
    PluginRegistry::new()
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Constructs a single `LogEntry` with the given source, level, and message.
pub fn make_log_entry(source: &str, level: LogLevel, msg: &str) -> LogEntry {
    LogEntry {
        timestamp: Utc::now(),
        level,
        source: source.to_string(),
        message: msg.to_string(),
        metadata: None,
    }
}

/// Constructs `n` `LogEntry` items with sequential messages and Info level.
pub fn make_log_entries(n: usize) -> Vec<LogEntry> {
    (0..n)
        .map(|i| make_log_entry("test-source", LogLevel::Info, &format!("message-{i}")))
        .collect()
}

// ---------------------------------------------------------------------------
// TestPlugin implementation
// ---------------------------------------------------------------------------

/// Configurable test plugin for use in plugin registry tests.
///
/// Controls whether the plugin reports success or failure, and whether
/// it emits a fatal condition.
pub struct TestPlugin {
    plugin_name: String,
    hooks: Vec<PluginHookType>,
    succeed: bool,
    fatal: bool,
}

impl TestPlugin {
    /// Creates a new `TestPlugin` with the given behavior.
    pub fn new(name: &str, hooks: Vec<PluginHookType>, succeed: bool, fatal: bool) -> Self {
        Self {
            plugin_name: name.to_string(),
            hooks,
            succeed,
            fatal,
        }
    }
}

#[async_trait]
impl PluginService for TestPlugin {
    async fn run_function(
        &self,
        _function_name: &str,
        _input: &[u8],
        _context: HashMap<String, String>,
    ) -> Result<PluginResult, ApiError> {
        let mut conditions = Vec::new();
        if self.fatal {
            conditions.push(PluginCondition {
                condition_type: "fatal".to_string(),
                message: "fatal error from test plugin".to_string(),
                severity: ConditionSeverity::Error,
            });
        }
        Ok(PluginResult {
            success: self.succeed,
            output: b"test-output".to_vec(),
            conditions,
        })
    }

    fn name(&self) -> &str {
        &self.plugin_name
    }

    fn hook_types(&self) -> &[PluginHookType] {
        &self.hooks
    }

    async fn health_check(&self) -> bool {
        self.succeed
    }
}

/// Creates a successful test plugin for the given hooks.
pub fn test_plugin_success(name: &str, hooks: Vec<PluginHookType>) -> Arc<dyn PluginService> {
    Arc::new(TestPlugin::new(name, hooks, true, false))
}

/// Creates a test plugin that returns a fatal condition (severity Error).
pub fn test_plugin_fatal(name: &str, hooks: Vec<PluginHookType>) -> Arc<dyn PluginService> {
    Arc::new(TestPlugin::new(name, hooks, false, true))
}
