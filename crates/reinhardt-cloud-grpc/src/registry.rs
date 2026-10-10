//! Agent registry for tracking connected cluster agents.
//!
//! The control plane maintains a registry of all connected agents,
//! their health status, and command channels for dispatching operations.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use dashmap::mapref::entry::Entry;
use thiserror::Error;
use tokio::sync::mpsc;
use tracing::{info, warn};
use uuid::Uuid;

use reinhardt_cloud_types::agent::{AgentCommand, AgentHealth, AgentInfo};

/// Default heartbeat interval expected from agents.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

/// Number of missed heartbeats before an agent is considered dead.
const MISSED_HEARTBEATS_THRESHOLD: u32 = 3;

/// Why an agent-supplied identity was refused by a cluster-scoped operation.
///
/// The variants deliberately carry no identifiers of other clusters so the
/// error can be surfaced to the caller without disclosing registry state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ClusterBindingError {
	/// No agent with the supplied identity is registered.
	#[error("agent is not registered")]
	NotRegistered,
	/// The agent is registered, but not under the authenticated cluster.
	#[error("agent is not bound to the authenticated cluster")]
	ClusterMismatch,
}

/// State of a connected agent.
struct AgentConnection {
	info: AgentInfo,
	health: Option<AgentHealth>,
	command_tx: mpsc::Sender<AgentCommand>,
	last_heartbeat: DateTime<Utc>,
	/// Optional cluster ID associated with this agent. Populated when an
	/// agent connects with an authenticated agent JWT (whose claims carry
	/// a `cluster_id`). Used by `ClusterAgentRegistry` to route commands
	/// addressed to a cluster instead of a specific agent.
	cluster_id: Option<Uuid>,
}

/// Registry tracking all connected cluster agents.
///
/// Thread-safe via `DashMap` for concurrent access from
/// gRPC handlers, health check tasks, and admin endpoints.
pub struct AgentRegistry {
	agents: Arc<DashMap<Uuid, AgentConnection>>,
}

impl Default for AgentRegistry {
	fn default() -> Self {
		Self::new()
	}
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
	pub fn register(&self, info: AgentInfo) -> mpsc::Receiver<AgentCommand> {
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
				cluster_id: None,
			},
		);

		rx
	}

	/// Register a new agent connection associated with a specific cluster.
	///
	/// Identical to [`AgentRegistry::register`] but also records the
	/// `cluster_id` taken from the agent's authenticated JWT claims, so
	/// later calls to [`AgentRegistry::send_command_to_cluster`] can route
	/// by cluster identity rather than agent identity.
	///
	/// The agent identity is supplied by the peer, so it cannot be trusted
	/// to be unclaimed. The check and the insert are one atomic map
	/// operation: an `agent_id` that is already registered under a different
	/// cluster (or without a cluster binding) is refused with
	/// [`ClusterBindingError::ClusterMismatch`] and the existing entry is
	/// left untouched. Re-registering an `agent_id` under the same cluster
	/// replaces the previous connection (agent reconnect).
	pub fn register_with_cluster(
		&self,
		info: AgentInfo,
		cluster_id: Uuid,
	) -> Result<mpsc::Receiver<AgentCommand>, ClusterBindingError> {
		let (tx, rx) = mpsc::channel(64);
		let agent_id = info.agent_id;
		let cluster_name = info.cluster_name.clone();
		let connection = AgentConnection {
			info,
			health: None,
			command_tx: tx,
			last_heartbeat: Utc::now(),
			cluster_id: Some(cluster_id),
		};

		match self.agents.entry(agent_id) {
			Entry::Occupied(mut occupied) => {
				if occupied.get().cluster_id != Some(cluster_id) {
					warn!(
						agent_id = %agent_id,
						cluster_id = %cluster_id,
						"Refused registration of an agent_id bound to another cluster"
					);
					return Err(ClusterBindingError::ClusterMismatch);
				}
				occupied.insert(connection);
			}
			Entry::Vacant(vacant) => {
				vacant.insert(connection);
			}
		}

		info!(
			agent_id = %agent_id,
			cluster_id = %cluster_id,
			cluster = %cluster_name,
			"Agent registered with cluster binding"
		);

		Ok(rx)
	}

	/// Send a command to any connected agent that reports itself as
	/// belonging to the given `cluster_id`.
	///
	/// Returns `Err` when no agent is currently registered for the
	/// cluster, or when the agent's command channel is closed.
	pub async fn send_command_to_cluster(
		&self,
		cluster_id: &Uuid,
		command: AgentCommand,
	) -> Result<(), String> {
		// Find the first agent whose cluster binding matches.
		let agent_id = self
			.agents
			.iter()
			.find(|entry| entry.cluster_id == Some(*cluster_id))
			.map(|entry| *entry.key())
			.ok_or_else(|| format!("No agent connected for cluster {cluster_id}"))?;

		self.send_command(&agent_id, command).await
	}

	/// List agent IDs currently bound to the given cluster.
	pub fn agents_for_cluster(&self, cluster_id: &Uuid) -> Vec<Uuid> {
		self.agents
			.iter()
			.filter(|entry| entry.cluster_id == Some(*cluster_id))
			.map(|entry| *entry.key())
			.collect()
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

	/// Update the health status of an agent on behalf of an authenticated
	/// cluster.
	///
	/// The health payload names the agent it describes, but that name is
	/// supplied by the peer. The update is applied only when
	/// `health.agent_id` is registered under `cluster_id`; otherwise the
	/// registry is left unchanged and the reason is returned.
	pub fn update_health_for_cluster(
		&self,
		cluster_id: &Uuid,
		health: AgentHealth,
	) -> Result<(), ClusterBindingError> {
		let mut conn = self
			.agents
			.get_mut(&health.agent_id)
			.ok_or(ClusterBindingError::NotRegistered)?;
		if conn.cluster_id != Some(*cluster_id) {
			return Err(ClusterBindingError::ClusterMismatch);
		}
		conn.health = Some(health);
		conn.last_heartbeat = Utc::now();
		Ok(())
	}

	/// Send a command to a specific agent.
	pub async fn send_command(&self, agent_id: &Uuid, command: AgentCommand) -> Result<(), String> {
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
			let timeout = HEARTBEAT_INTERVAL * MISSED_HEARTBEATS_THRESHOLD;
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

	/// Set last_heartbeat for testing time-dependent behavior.
	#[cfg(test)]
	pub fn set_last_heartbeat_for_test(&self, agent_id: &Uuid, time: DateTime<Utc>) {
		if let Some(mut conn) = self.agents.get_mut(agent_id) {
			conn.last_heartbeat = time;
		}
	}

	/// Evict agents that have missed too many heartbeats.
	pub fn evict_stale_agents(&self) -> Vec<Uuid> {
		let timeout = HEARTBEAT_INTERVAL * MISSED_HEARTBEATS_THRESHOLD;
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
		let id = Uuid::now_v7();
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
		let id = Uuid::now_v7();
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
		let id = Uuid::now_v7();
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
		let id = Uuid::now_v7();
		let mut rx = registry.register(test_agent_info(id));

		// Act
		let cmd = AgentCommand::Restart {
			project_name: "web".to_string(),
		};
		registry.send_command(&id, cmd).await.unwrap();

		// Assert
		let received = rx.recv().await.unwrap();
		assert!(
			matches!(received, AgentCommand::Restart { project_name } if project_name == "web")
		);
	}

	#[rstest]
	fn test_update_health() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::now_v7();
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

	#[rstest]
	#[tokio::test]
	async fn test_send_command_to_nonexistent_agent() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::now_v7();
		let cmd = AgentCommand::Restart {
			project_name: "web".to_string(),
		};

		// Act
		let result = registry.send_command(&id, cmd).await;

		// Assert
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("not connected"));
	}

	#[rstest]
	#[tokio::test]
	async fn test_send_command_channel_closed() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::now_v7();
		let rx = registry.register(test_agent_info(id));
		drop(rx); // close the receiver side

		let cmd = AgentCommand::Restart {
			project_name: "web".to_string(),
		};

		// Act
		let result = registry.send_command(&id, cmd).await;

		// Assert
		assert!(result.is_err());
		assert!(result.unwrap_err().contains("channel closed"));
	}

	#[rstest]
	fn test_get_health_nonexistent_agent() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::now_v7();

		// Act
		let result = registry.get_health(&id);

		// Assert
		assert!(result.is_none());
	}

	#[rstest]
	fn test_is_healthy_nonexistent_agent() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::now_v7();

		// Act
		let result = registry.is_healthy(&id);

		// Assert
		assert!(!result);
	}

	#[rstest]
	fn test_register_same_agent_twice() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::now_v7();
		let _rx1 = registry.register(test_agent_info(id));

		// Act — register same agent_id again
		let _rx2 = registry.register(test_agent_info(id));

		// Assert — second registration replaces first, count remains 1
		assert_eq!(registry.count(), 1);
		let agents = registry.list_connected();
		assert_eq!(agents.len(), 1);
		assert_eq!(agents[0].agent_id, id);
	}

	#[rstest]
	fn test_unregister_nonexistent_agent() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::now_v7();
		let other = Uuid::now_v7();
		let _rx = registry.register(test_agent_info(id));

		// Act — unregister a non-existent agent
		registry.unregister(&other);

		// Assert — no panic, count unchanged
		assert_eq!(registry.count(), 1);
	}

	#[rstest]
	fn test_heartbeat_nonexistent_agent() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::now_v7();

		// Act — heartbeat for non-existent agent
		registry.heartbeat(&id);

		// Assert — no panic
		assert_eq!(registry.count(), 0);
	}

	#[rstest]
	#[tokio::test]
	async fn test_agent_lifecycle_full() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::now_v7();
		let mut rx = registry.register(test_agent_info(id));

		// Act — heartbeat
		registry.heartbeat(&id);
		assert!(registry.is_healthy(&id));

		// Act — update health
		let health = AgentHealth {
			agent_id: id,
			healthy: true,
			cpu_usage_percent: 30.0,
			memory_usage_percent: 50.0,
			pod_count: 5,
			reported_at: Utc::now(),
		};
		registry.update_health(&id, health);

		// Assert — health stored
		assert!(registry.is_healthy(&id));
		let stored = registry.get_health(&id).unwrap();
		assert_eq!(stored.pod_count, 5);

		// Act — send a command
		let cmd = AgentCommand::Scale {
			project_name: "api".to_string(),
			replicas: 3,
		};
		registry.send_command(&id, cmd).await.unwrap();
		let received = rx.recv().await.unwrap();
		assert!(matches!(received, AgentCommand::Scale { replicas: 3, .. }));

		// Act — unregister
		registry.unregister(&id);

		// Assert
		assert_eq!(registry.count(), 0);
	}

	#[rstest]
	fn test_evict_stale_agents() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::now_v7();
		let _rx = registry.register(test_agent_info(id));

		// Set heartbeat to 2 minutes ago (well past 90s threshold)
		let two_min_ago = Utc::now() - chrono::Duration::seconds(120);
		registry.set_last_heartbeat_for_test(&id, two_min_ago);

		// Act
		let evicted = registry.evict_stale_agents();

		// Assert
		assert_eq!(evicted.len(), 1);
		assert_eq!(evicted[0], id);
		assert_eq!(registry.count(), 0);
	}

	#[rstest]
	fn test_is_healthy_heartbeat_boundary() {
		// Arrange
		let registry = AgentRegistry::new();
		let id = Uuid::now_v7();
		let _rx = registry.register(test_agent_info(id));

		// Just under 90s ago -> healthy
		let just_under = Utc::now() - chrono::Duration::seconds(89);
		registry.set_last_heartbeat_for_test(&id, just_under);

		// Assert
		assert!(registry.is_healthy(&id));

		// Exactly 90s ago -> unhealthy (elapsed >= timeout)
		let exactly_90 = Utc::now() - chrono::Duration::seconds(90);
		registry.set_last_heartbeat_for_test(&id, exactly_90);

		// Assert
		assert!(!registry.is_healthy(&id));

		// Over 90s ago -> unhealthy
		let over_90 = Utc::now() - chrono::Duration::seconds(120);
		registry.set_last_heartbeat_for_test(&id, over_90);

		// Assert
		assert!(!registry.is_healthy(&id));
	}

	#[rstest]
	#[tokio::test]
	async fn test_multiple_agents_independent_channels() {
		// Arrange
		let registry = AgentRegistry::new();
		let id1 = Uuid::now_v7();
		let id2 = Uuid::now_v7();
		let id3 = Uuid::now_v7();
		let mut rx1 = registry.register(test_agent_info(id1));
		let mut rx2 = registry.register(test_agent_info(id2));
		let mut rx3 = registry.register(test_agent_info(id3));

		// Act — send different commands to each agent
		registry
			.send_command(
				&id1,
				AgentCommand::Deploy {
					project_name: "app1".to_string(),
					image: "img1".to_string(),
					replicas: 1,
				},
			)
			.await
			.unwrap();

		registry
			.send_command(
				&id2,
				AgentCommand::Scale {
					project_name: "app2".to_string(),
					replicas: 5,
				},
			)
			.await
			.unwrap();

		registry
			.send_command(
				&id3,
				AgentCommand::Restart {
					project_name: "app3".to_string(),
				},
			)
			.await
			.unwrap();

		// Assert — each agent receives its own command
		let cmd1 = rx1.recv().await.unwrap();
		assert!(
			matches!(cmd1, AgentCommand::Deploy { project_name, .. } if project_name == "app1")
		);

		let cmd2 = rx2.recv().await.unwrap();
		assert!(matches!(cmd2, AgentCommand::Scale { project_name, .. } if project_name == "app2"));

		let cmd3 = rx3.recv().await.unwrap();
		assert!(matches!(cmd3, AgentCommand::Restart { project_name } if project_name == "app3"));

		assert_eq!(registry.count(), 3);
	}

	#[rstest]
	#[tokio::test]
	async fn test_concurrent_register_unregister() {
		// Arrange
		let registry = std::sync::Arc::new(AgentRegistry::new());
		let mut handles = vec![];

		// Act — spawn 10 register/unregister tasks concurrently
		for _ in 0..10 {
			let reg = registry.clone();
			handles.push(tokio::spawn(async move {
				let id = Uuid::now_v7();
				let _rx = reg.register(AgentInfo {
					agent_id: id,
					cluster_name: "test".to_string(),
					node_name: "node".to_string(),
					version: "0.1.0".to_string(),
					last_seen: Utc::now(),
				});
				reg.heartbeat(&id);
				reg.unregister(&id);
			}));
		}

		for handle in handles {
			handle.await.unwrap();
		}

		// Assert — all agents have been unregistered, no panic
		assert_eq!(registry.count(), 0);
	}

	// --- Cluster-aware routing tests ---

	#[rstest]
	#[tokio::test]
	async fn test_register_with_cluster_binds_agent_to_cluster() {
		// Arrange
		let registry = AgentRegistry::new();
		let agent_id = Uuid::now_v7();
		let cluster_id = Uuid::now_v7();

		// Act
		let _rx = registry
			.register_with_cluster(test_agent_info(agent_id), cluster_id)
			.unwrap();

		// Assert
		let agents = registry.agents_for_cluster(&cluster_id);
		assert_eq!(agents.len(), 1);
		assert_eq!(agents[0], agent_id);
	}

	#[rstest]
	#[tokio::test]
	async fn test_send_command_to_cluster_routes_to_right_agent() {
		// Arrange
		let registry = AgentRegistry::new();
		let agent_id = Uuid::now_v7();
		let cluster_id = Uuid::now_v7();
		let mut rx = registry
			.register_with_cluster(test_agent_info(agent_id), cluster_id)
			.unwrap();

		// Act
		let cmd = AgentCommand::Deploy {
			project_name: "web".to_string(),
			image: "web:v1".to_string(),
			replicas: 3,
		};
		registry
			.send_command_to_cluster(&cluster_id, cmd.clone())
			.await
			.unwrap();

		// Assert
		let received = rx.recv().await.unwrap();
		assert_eq!(received, cmd);
	}

	#[rstest]
	#[tokio::test]
	async fn test_send_command_to_unknown_cluster_fails() {
		// Arrange
		let registry = AgentRegistry::new();
		let unknown_cluster = Uuid::now_v7();

		// Act
		let result = registry
			.send_command_to_cluster(
				&unknown_cluster,
				AgentCommand::Restart {
					project_name: "x".to_string(),
				},
			)
			.await;

		// Assert
		assert!(result.is_err());
	}

	// --- Cluster binding enforcement tests ---

	fn test_health(agent_id: Uuid, pod_count: u32) -> AgentHealth {
		AgentHealth {
			agent_id,
			healthy: true,
			cpu_usage_percent: 1.0,
			memory_usage_percent: 2.0,
			pod_count,
			reported_at: Utc::now(),
		}
	}

	#[rstest]
	fn test_register_with_cluster_refuses_agent_id_bound_to_other_cluster() {
		// Arrange
		let registry = AgentRegistry::new();
		let agent_id = Uuid::now_v7();
		let victim_cluster = Uuid::now_v7();
		let attacker_cluster = Uuid::now_v7();
		let _victim_rx = registry
			.register_with_cluster(test_agent_info(agent_id), victim_cluster)
			.unwrap();

		// Act
		let result = registry.register_with_cluster(test_agent_info(agent_id), attacker_cluster);

		// Assert
		assert_eq!(result.unwrap_err(), ClusterBindingError::ClusterMismatch);
		assert_eq!(registry.agents_for_cluster(&victim_cluster), vec![agent_id]);
		assert_eq!(
			registry.agents_for_cluster(&attacker_cluster),
			Vec::<Uuid>::new()
		);
	}

	#[rstest]
	fn test_register_with_cluster_refuses_agent_id_registered_without_binding() {
		// Arrange
		let registry = AgentRegistry::new();
		let agent_id = Uuid::now_v7();
		let cluster_id = Uuid::now_v7();
		let _rx = registry.register(test_agent_info(agent_id));

		// Act
		let result = registry.register_with_cluster(test_agent_info(agent_id), cluster_id);

		// Assert
		assert_eq!(result.unwrap_err(), ClusterBindingError::ClusterMismatch);
		assert_eq!(registry.agents_for_cluster(&cluster_id), Vec::<Uuid>::new());
	}

	#[rstest]
	#[tokio::test]
	async fn test_register_with_cluster_allows_reconnect_in_same_cluster() {
		// Arrange
		let registry = AgentRegistry::new();
		let agent_id = Uuid::now_v7();
		let cluster_id = Uuid::now_v7();
		let _old_rx = registry
			.register_with_cluster(test_agent_info(agent_id), cluster_id)
			.unwrap();

		// Act
		let mut new_rx = registry
			.register_with_cluster(test_agent_info(agent_id), cluster_id)
			.unwrap();
		let cmd = AgentCommand::Restart {
			project_name: "web".to_string(),
		};
		registry.send_command(&agent_id, cmd.clone()).await.unwrap();

		// Assert
		assert_eq!(registry.count(), 1);
		assert_eq!(new_rx.recv().await.unwrap(), cmd);
	}

	#[rstest]
	fn test_update_health_for_cluster_accepts_own_agent() {
		// Arrange
		let registry = AgentRegistry::new();
		let agent_id = Uuid::now_v7();
		let cluster_id = Uuid::now_v7();
		let _rx = registry
			.register_with_cluster(test_agent_info(agent_id), cluster_id)
			.unwrap();

		// Act
		let result = registry.update_health_for_cluster(&cluster_id, test_health(agent_id, 7));

		// Assert
		assert_eq!(result, Ok(()));
		assert_eq!(registry.get_health(&agent_id).unwrap().pod_count, 7);
	}

	#[rstest]
	fn test_update_health_for_cluster_rejects_other_clusters_agent() {
		// Arrange
		let registry = AgentRegistry::new();
		let agent_id = Uuid::now_v7();
		let victim_cluster = Uuid::now_v7();
		let attacker_cluster = Uuid::now_v7();
		let _rx = registry
			.register_with_cluster(test_agent_info(agent_id), victim_cluster)
			.unwrap();
		registry
			.update_health_for_cluster(&victim_cluster, test_health(agent_id, 3))
			.unwrap();

		// Act
		let result =
			registry.update_health_for_cluster(&attacker_cluster, test_health(agent_id, 99));

		// Assert
		assert_eq!(result, Err(ClusterBindingError::ClusterMismatch));
		assert_eq!(registry.get_health(&agent_id).unwrap().pod_count, 3);
	}

	#[rstest]
	fn test_update_health_for_cluster_rejects_unregistered_agent() {
		// Arrange
		let registry = AgentRegistry::new();
		let agent_id = Uuid::now_v7();

		// Act
		let result = registry.update_health_for_cluster(&Uuid::now_v7(), test_health(agent_id, 1));

		// Assert
		assert_eq!(result, Err(ClusterBindingError::NotRegistered));
		assert!(registry.get_health(&agent_id).is_none());
	}
}
