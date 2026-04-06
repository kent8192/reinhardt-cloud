//! Mock service implementations for testing.
//!
//! Provides configurable mock implementations of all service traits
//! for use in unit and integration tests.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio_stream::Stream;
use uuid::Uuid;

use crate::auth::Claims;
use crate::error::ApiError;
use crate::pagination::{PaginatedResponse, PaginationParams};
use crate::traits::{AuthService, BuildService, ClusterAgentService, LogService};
use reinhardt_cloud_types::User;
use reinhardt_cloud_types::agent::{AgentCommand, AgentEvent, AgentHealth};
use reinhardt_cloud_types::build::{BuildEvent, BuildRequest, BuildStatus};
use reinhardt_cloud_types::log::{LogEntry, LogFilter};

// --- MockAuthService ---

/// Mock authentication service with configurable behavior.
pub struct MockAuthService {
	authenticate_result: Arc<Mutex<Result<Claims, ApiError>>>,
	verify_result: Arc<Mutex<Result<Claims, ApiError>>>,
	user_info_result: Arc<Mutex<Result<User, ApiError>>>,
}

impl MockAuthService {
	/// Create a new mock with default success responses.
	pub fn new() -> Self {
		let default_claims = Claims {
			sub: Uuid::new_v4().to_string(),
			username: "test-user".to_string(),
			exp: chrono::Utc::now().timestamp() + 86400,
			iat: chrono::Utc::now().timestamp(),
		};
		let default_user = User::new("test-user", "test@example.com", "hash");

		Self {
			authenticate_result: Arc::new(Mutex::new(Ok(default_claims.clone()))),
			verify_result: Arc::new(Mutex::new(Ok(default_claims))),
			user_info_result: Arc::new(Mutex::new(Ok(default_user))),
		}
	}

	/// Configure the result returned by `authenticate`.
	pub async fn set_authenticate_result(&self, result: Result<Claims, ApiError>) {
		*self.authenticate_result.lock().await = result;
	}

	/// Configure the result returned by `verify_token`.
	pub async fn set_verify_result(&self, result: Result<Claims, ApiError>) {
		*self.verify_result.lock().await = result;
	}

	/// Configure the result returned by `get_user_info`.
	pub async fn set_user_info_result(&self, result: Result<User, ApiError>) {
		*self.user_info_result.lock().await = result;
	}
}

impl Default for MockAuthService {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl AuthService for MockAuthService {
	async fn authenticate(&self, _username: &str, _password: &str) -> Result<Claims, ApiError> {
		self.authenticate_result.lock().await.clone()
	}

	async fn create_token(&self, user_id: &str, username: &str) -> Result<String, ApiError> {
		Ok(format!("mock-token-{user_id}-{username}"))
	}

	async fn verify_token(&self, _token: &str) -> Result<Claims, ApiError> {
		self.verify_result.lock().await.clone()
	}

	async fn get_user_info(&self, _user_id: &str) -> Result<User, ApiError> {
		self.user_info_result.lock().await.clone()
	}
}

// --- MockBuildService ---

/// Mock build service with configurable behavior.
pub struct MockBuildService {
	build_status_result: Arc<Mutex<Result<BuildStatus, ApiError>>>,
}

impl MockBuildService {
	/// Create a new mock with default success responses.
	pub fn new() -> Self {
		let default_status = BuildStatus {
			build_id: Uuid::new_v4(),
			app_name: "test-app".to_string(),
			phase: reinhardt_cloud_types::build::BuildPhase::Building,
			completed: false,
			success: None,
			started_at: chrono::Utc::now(),
			completed_at: None,
		};

		Self {
			build_status_result: Arc::new(Mutex::new(Ok(default_status))),
		}
	}

	/// Configure the result returned by `get_build_status`.
	pub async fn set_build_status_result(&self, result: Result<BuildStatus, ApiError>) {
		*self.build_status_result.lock().await = result;
	}
}

impl Default for MockBuildService {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl BuildService for MockBuildService {
	async fn start_build(
		&self,
		_request: BuildRequest,
	) -> Result<Pin<Box<dyn Stream<Item = Result<BuildEvent, ApiError>> + Send>>, ApiError> {
		let events = vec![
			Ok(BuildEvent::PhaseChange {
				phase: reinhardt_cloud_types::build::BuildPhase::Building,
				timestamp: chrono::Utc::now(),
			}),
			Ok(BuildEvent::Complete {
				success: true,
				timestamp: chrono::Utc::now(),
			}),
		];
		Ok(Box::pin(tokio_stream::iter(events)))
	}

	async fn cancel_build(&self, _build_id: Uuid) -> Result<(), ApiError> {
		Ok(())
	}

	async fn get_build_status(&self, _build_id: Uuid) -> Result<BuildStatus, ApiError> {
		self.build_status_result.lock().await.clone()
	}
}

// --- MockClusterAgentService ---

/// Mock cluster agent service.
pub struct MockClusterAgentService;

impl MockClusterAgentService {
	pub fn new() -> Self {
		Self
	}
}

impl Default for MockClusterAgentService {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl ClusterAgentService for MockClusterAgentService {
	async fn agent_stream(
		&self,
		_agent_events: Pin<Box<dyn Stream<Item = Result<AgentEvent, ApiError>> + Send>>,
	) -> Result<Pin<Box<dyn Stream<Item = Result<AgentCommand, ApiError>> + Send>>, ApiError> {
		let commands: Vec<Result<AgentCommand, ApiError>> = vec![];
		Ok(Box::pin(tokio_stream::iter(commands)))
	}

	async fn report_health(&self, _health: AgentHealth) -> Result<(), ApiError> {
		Ok(())
	}

	async fn get_agent_health(&self, agent_id: Uuid) -> Result<AgentHealth, ApiError> {
		Ok(AgentHealth {
			agent_id,
			healthy: true,
			cpu_usage_percent: 10.0,
			memory_usage_percent: 25.0,
			pod_count: 5,
			reported_at: chrono::Utc::now(),
		})
	}
}

// --- MockLogService ---

/// Mock log service with configurable behavior.
pub struct MockLogService {
	logs: Arc<Mutex<Vec<LogEntry>>>,
}

impl MockLogService {
	pub fn new() -> Self {
		Self {
			logs: Arc::new(Mutex::new(Vec::new())),
		}
	}

	/// Get all pushed logs (for test assertions).
	pub async fn get_pushed_logs(&self) -> Vec<LogEntry> {
		self.logs.lock().await.clone()
	}
}

impl Default for MockLogService {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl LogService for MockLogService {
	async fn push_logs(&self, entries: Vec<LogEntry>) -> Result<(), ApiError> {
		self.logs.lock().await.extend(entries);
		Ok(())
	}

	async fn tail_logs(
		&self,
		_filter: LogFilter,
	) -> Result<Pin<Box<dyn Stream<Item = Result<LogEntry, ApiError>> + Send>>, ApiError> {
		let entries: Vec<Result<LogEntry, ApiError>> = vec![];
		Ok(Box::pin(tokio_stream::iter(entries)))
	}

	async fn list_logs(
		&self,
		_filter: LogFilter,
		pagination: PaginationParams,
	) -> Result<PaginatedResponse<LogEntry>, ApiError> {
		let logs = self.logs.lock().await.clone();
		let total = logs.len() as u64;
		Ok(PaginatedResponse::new(logs, total, &pagination))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	use tokio_stream::StreamExt;

	#[rstest]
	#[tokio::test]
	async fn test_mock_auth_service_defaults() {
		// Arrange
		let service = MockAuthService::new();

		// Act
		let claims = service.authenticate("user", "pass").await.unwrap();
		let token = service.create_token("123", "user").await.unwrap();
		let verified = service.verify_token("any-token").await.unwrap();

		// Assert
		assert_eq!(claims.username, "test-user");
		assert!(token.contains("123"));
		assert_eq!(verified.username, "test-user");
	}

	#[rstest]
	#[tokio::test]
	async fn test_mock_auth_service_configurable_error() {
		// Arrange
		let service = MockAuthService::new();
		service
			.set_authenticate_result(Err(ApiError::Unauthorized("denied".to_string())))
			.await;

		// Act
		let result = service.authenticate("user", "pass").await;

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	#[tokio::test]
	async fn test_mock_build_service_start_build() {
		// Arrange
		let service = MockBuildService::new();
		let request = BuildRequest {
			app_name: "test".to_string(),
			image: "test:latest".to_string(),
			env_vars: vec![],
			dockerfile: None,
			context_path: None,
		};

		// Act
		let mut stream = service.start_build(request).await.unwrap();
		let mut events = vec![];
		while let Some(event) = stream.next().await {
			events.push(event.unwrap());
		}

		// Assert
		assert_eq!(events.len(), 2);
		assert!(matches!(
			events[1],
			BuildEvent::Complete { success: true, .. }
		));
	}

	#[rstest]
	#[tokio::test]
	async fn test_mock_log_service_push_and_retrieve() {
		// Arrange
		let service = MockLogService::new();
		let entry = LogEntry {
			timestamp: chrono::Utc::now(),
			level: reinhardt_cloud_types::log::LogLevel::Info,
			source: "test".to_string(),
			message: "hello".to_string(),
			metadata: None,
		};

		// Act
		service.push_logs(vec![entry.clone()]).await.unwrap();
		let logs = service.get_pushed_logs().await;

		// Assert
		assert_eq!(logs.len(), 1);
		assert_eq!(logs[0].message, "hello");
	}

	#[rstest]
	#[tokio::test]
	async fn test_mock_cluster_agent_service() {
		// Arrange
		let service = MockClusterAgentService::new();
		let agent_id = Uuid::new_v4();

		// Act
		let health = service.get_agent_health(agent_id).await.unwrap();
		service.report_health(health.clone()).await.unwrap();

		// Assert
		assert!(health.healthy);
		assert_eq!(health.agent_id, agent_id);
	}
}
