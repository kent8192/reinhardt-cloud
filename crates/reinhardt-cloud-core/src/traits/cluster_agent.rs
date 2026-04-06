//! Cluster agent service trait.

use std::pin::Pin;

use async_trait::async_trait;
use tokio_stream::Stream;
use uuid::Uuid;

use crate::error::ApiError;
use reinhardt_cloud_types::agent::{AgentCommand, AgentEvent, AgentHealth, DeployStatusReport};

/// Trait for bidirectional communication with cluster agents.
///
/// Provides streaming communication channels between the control plane
/// and remote cluster agents, plus health reporting.
#[async_trait]
pub trait ClusterAgentService: Send + Sync + 'static {
	/// Open a bidirectional stream with an agent.
	///
	/// Accepts a stream of events from the agent and returns a stream
	/// of commands to send back.
	async fn agent_stream(
		&self,
		agent_events: Pin<Box<dyn Stream<Item = Result<AgentEvent, ApiError>> + Send>>,
	) -> Result<Pin<Box<dyn Stream<Item = Result<AgentCommand, ApiError>> + Send>>, ApiError>;

	/// Report health status for an agent.
	async fn report_health(&self, health: AgentHealth) -> Result<(), ApiError>;

	/// Get health status for an agent by ID.
	async fn get_agent_health(&self, agent_id: Uuid) -> Result<AgentHealth, ApiError>;

	/// Report the status of a deployment operation.
	async fn report_deploy_status(&self, report: DeployStatusReport) -> Result<(), ApiError>;
}
