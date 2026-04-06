//! Agent registry for tracking connected cluster agents.
//!
//! The control plane maintains a registry of all connected agents,
//! their health status, and command channels for dispatching operations.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use tokio::sync::mpsc;
use tracing::{info, warn};
use uuid::Uuid;

use reinhardt_cloud_types::agent::{AgentCommand, AgentHealth, AgentInfo};

/// Default heartbeat interval expected from agents.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Number of missed heartbeats before an agent is considered dead.
const MISSED_HEARTBEATS_THRESHOLD: u32 = 3;

/// State of a connected agent.
struct AgentConnection {
	info: AgentInfo,
	health: Option<AgentHealth>,
	command_tx: mpsc::Sender<AgentCommand>,
	last_heartbeat: DateTime<Utc>,
}

/// Registry tracking all connected cluster agents.
///
/// Thread-safe via `DashMap` for concurrent access from
/// gRPC handlers, health check tasks, and admin endpoints.
pub struct AgentRegistry {
	agents: Arc<DashMap<Uuid, AgentConnection>>,
}

impl AgentRegistry {
	pub fn new() -> Self {
		Self {
			agents: Arc::new(DashMap::new()),
		}
	}

	/// Register a new agent connection.
	///
	/// Returns a receiver for commands to send to the agent.
	pub fn register(
		&self,
		info: AgentInfo,
	) -> mpsc::Receiver<AgentCommand> {
		let (tx, rx) = mpsc::channel(64);
		let agent_id = info.agent_id;

		info!(
			agent_id = %agent_id,
			cluster = %info.cluster_name,
			"Agent registered"
		);

		self.agents.insert(
			agent_id,
			AgentConnection {
				info,
				health: None,
				command_tx: tx,
				last_heartbeat: Utc::now(),
			},
		);

		rx
	}

	/// Remove an agent from the registry.
	pub fn unregister(&self, agent_id: &Uuid) {
		if self.agents.remove(agent_id).is_some() {
			info!(agent_id = %agent_id, "Agent unregistered");
		}
	}

	/// Update the last heartbeat timestamp for an agent.
	pub fn heartbeat(&self, agent_id: &Uuid) {
		if let Some(mut conn) = self.agents.get_mut(agent_id) {
			conn.last_heartbeat = Utc::now();
		}
	}

	/// Update the health status for an agent.
	pub fn update_health(&self, agent_id: &Uuid, health: AgentHealth) {
		if let Some(mut conn) = self.agents.get_mut(agent_id) {
			conn.health = Some(health);
			conn.last_heartbeat = Utc::now();
		}
	}

	/// Send a command to a specific agent.
	pub async fn send_command(
		&self,
		agent_id: &Uuid,
		command: AgentCommand,
	) -> Result<(), String> {
		let conn = self
			.agents
			.get(agent_id)
			.ok_or_else(|| format!("Agent {agent_id} not connected"))?;

		conn.command_tx
			.send(command)
			.await
			.map_err(|_| format!("Agent {agent_id} command channel closed"))
	}

	/// List all connected agents.
	pub fn list_connected(&self) -> Vec<AgentInfo> {
		self.agents.iter().map(|r| r.info.clone()).collect()
	}

	/// Check if an agent is healthy (heartbeat within threshold).
	pub fn is_healthy(&self, agent_id: &Uuid) -> bool {
		self.agents.get(agent_id).is_some_and(|conn| {
			let timeout =
				HEARTBEAT_INTERVAL * MISSED_HEARTBEATS_THRESHOLD;
			let elapsed = Utc::now()
				.signed_duration_since(conn.last_heartbeat)
				.to_std()
				.unwrap_or(Duration::MAX);
			elapsed < timeout
		})
	}

	/// Get the health report for an agent.
	pub fn get_health(&self, agent_id: &Uuid) -> Option<AgentHealth> {
		self.agents.get(agent_id).and_then(|c| c.health.clone())
	}

	/// Number of connected agents.
	pub fn count(&self) -> usize {
		self.agents.len()
	}

	/// Evict agents that have missed too many heartbeats.
	pub fn evict_stale_agents(&self) -> Vec<Uuid> {
		let timeout =
			HEARTBEAT_INTERVAL * MISSED_HEARTBEATS_THRESHOLD;
		let mut evicted = vec![];

		self.agents.retain(|id, conn| {
			let elapsed = Utc::now()
				.signed_duration_since(conn.last_heartbeat)
				.to_std()
				.unwrap_or(Duration::MAX);
			if elapsed >= timeout {
				warn!(agent_id = %id, "Evicting stale agent");
				evicted.push(*id);
				false
			} else {
				true
			}
		});

		evicted
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	fn test_agent_info(id: Uuid) -> AgentInfo {
		AgentInfo {
			agent_id: id,
			cluster_name: "test-cluster".to_string(),
			node_name: "node-01".to_string(),
			version: "0.1.0".to_string(),
			last_seen: Utc::now(),
		}
	}

	#[rstest]
	fn test_register_and_list() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::new_v4();
		let info = test_agent_info(id);

		// Act
		let _rx = registry.register(info);
		let agents = registry.list_connected();

		// Assert
		assert_eq!(agents.len(), 1);
		assert_eq!(agents[0].agent_id, id);
	}

	#[rstest]
	fn test_unregister() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::new_v4();
		let _rx = registry.register(test_agent_info(id));

		// Act
		registry.unregister(&id);

		// Assert
		assert_eq!(registry.count(), 0);
	}

	#[rstest]
	fn test_heartbeat_keeps_agent_healthy() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::new_v4();
		let _rx = registry.register(test_agent_info(id));

		// Act
		registry.heartbeat(&id);

		// Assert
		assert!(registry.is_healthy(&id));
	}

	#[rstest]
	#[tokio::test]
	async fn test_send_command() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::new_v4();
		let mut rx = registry.register(test_agent_info(id));

		// Act
		let cmd = AgentCommand::Restart {
			app_name: "web".to_string(),
		};
		registry.send_command(&id, cmd).await.unwrap();

		// Assert
		let received = rx.recv().await.unwrap();
		assert!(matches!(received, AgentCommand::Restart { app_name } if app_name == "web"));
	}

	#[rstest]
	fn test_update_health() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::new_v4();
		let _rx = registry.register(test_agent_info(id));
		let health = AgentHealth {
			agent_id: id,
			healthy: true,
			cpu_usage_percent: 42.0,
			memory_usage_percent: 55.0,
			pod_count: 10,
			reported_at: Utc::now(),
		};

		// Act
		registry.update_health(&id, health);

		// Assert
		let stored = registry.get_health(&id).unwrap();
		assert!((stored.cpu_usage_percent - 42.0).abs() < f64::EPSILON);
	}
}
