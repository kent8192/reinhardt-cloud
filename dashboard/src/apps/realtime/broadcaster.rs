//! WebSocket event broadcaster for distributing real-time updates.
//!
//! `WsBroadcaster` manages WebSocket connections and deployment subscriptions,
//! routing deployment status updates to subscribed users only while delivering
//! system notifications to all connected users.

use std::collections::HashSet;

use dashmap::DashMap;
use tokio::sync::mpsc;

use crate::shared::ws_messages::{DeploymentStatusPayload, SystemNotificationPayload, WsMessage};

/// Maximum number of deployment subscriptions allowed per user.
pub const MAX_SUBSCRIPTIONS_PER_USER: usize = 100;

/// Handle representing a single WebSocket connection.
///
/// Each user may have multiple concurrent connections (e.g. multiple browser
/// tabs). The `sender` is used to push serialized JSON messages to the
/// WebSocket write half.
pub struct ConnectionHandle {
	pub connection_id: String,
	pub user_id: String,
	pub sender: mpsc::UnboundedSender<String>,
}

/// Server-side broadcaster that fans out WebSocket messages to connected clients.
///
/// # Design
///
/// - **connections**: Maps `user_id` to a list of active `ConnectionHandle`s.
/// - **subscriptions**: Maps `deployment_id` to the set of subscribed `user_id`s.
/// - **user_subscriptions**: Maps `user_id` to the set of `deployment_id`s they follow.
///
/// All maps use `DashMap` for lock-free concurrent access from multiple tokio tasks.
pub struct WsBroadcaster {
	/// user_id -> list of active connections
	connections: DashMap<String, Vec<ConnectionHandle>>,
	/// deployment_id -> set of subscribed user_ids
	subscriptions: DashMap<String, HashSet<String>>,
	/// user_id -> set of deployment_ids the user is subscribed to
	user_subscriptions: DashMap<String, HashSet<String>>,
}

impl WsBroadcaster {
	/// Create a new empty broadcaster.
	pub fn new() -> Self {
		Self {
			connections: DashMap::new(),
			subscriptions: DashMap::new(),
			user_subscriptions: DashMap::new(),
		}
	}

	/// Register a new WebSocket connection for a user.
	pub fn register_connection(&self, handle: ConnectionHandle) {
		self.connections
			.entry(handle.user_id.clone())
			.or_default()
			.push(handle);
	}

	/// Remove a specific connection by id. If this was the user's last
	/// connection, their subscriptions are also cleaned up.
	pub fn remove_connection(&self, user_id: &str, connection_id: &str) {
		let should_cleanup = if let Some(mut conns) = self.connections.get_mut(user_id) {
			conns.retain(|c| c.connection_id != connection_id);
			conns.is_empty()
		} else {
			false
		};

		if should_cleanup {
			self.connections.remove(user_id);
			self.cleanup_subscriptions(user_id);
		}
	}

	/// Subscribe a user to deployment status updates.
	pub fn subscribe(&self, user_id: &str, deployment_id: &str) {
		self.subscriptions
			.entry(deployment_id.to_string())
			.or_default()
			.insert(user_id.to_string());

		self.user_subscriptions
			.entry(user_id.to_string())
			.or_default()
			.insert(deployment_id.to_string());
	}

	/// Subscribe a user to deployment status updates, enforcing the per-user
	/// subscription limit. Returns `false` if the limit would be exceeded.
	pub fn try_subscribe(&self, user_id: &str, deployment_id: &str) -> bool {
		// Check if already subscribed (does not count against limit)
		if let Some(subs) = self.user_subscriptions.get(user_id) {
			if subs.contains(deployment_id) {
				return true;
			}
			if subs.len() >= MAX_SUBSCRIPTIONS_PER_USER {
				return false;
			}
		}

		self.subscribe(user_id, deployment_id);
		true
	}

	/// Unsubscribe a user from a specific deployment.
	pub fn unsubscribe(&self, user_id: &str, deployment_id: &str) {
		if let Some(mut users) = self.subscriptions.get_mut(deployment_id) {
			users.remove(user_id);
			if users.is_empty() {
				drop(users);
				self.subscriptions.remove(deployment_id);
			}
		}

		if let Some(mut deps) = self.user_subscriptions.get_mut(user_id) {
			deps.remove(deployment_id);
			if deps.is_empty() {
				drop(deps);
				self.user_subscriptions.remove(user_id);
			}
		}
	}

	/// Check whether a user is subscribed to a deployment.
	pub fn is_subscribed(&self, user_id: &str, deployment_id: &str) -> bool {
		self.user_subscriptions
			.get(user_id)
			.map(|subs| subs.contains(deployment_id))
			.unwrap_or(false)
	}

	/// Total number of connected users (not individual connections).
	pub fn connection_count(&self) -> usize {
		self.connections.len()
	}

	/// Broadcast a deployment status update to all users subscribed to that
	/// deployment.
	pub fn broadcast_deployment_status(&self, payload: &DeploymentStatusPayload) {
		let msg = WsMessage::DeploymentStatus(payload.clone());
		let json = match serde_json::to_string(&msg) {
			Ok(j) => j,
			Err(_) => return,
		};

		if let Some(subscriber_ids) = self.subscriptions.get(&payload.deployment_id) {
			for user_id in subscriber_ids.iter() {
				self.send_to_user(user_id, &json);
			}
		}
	}

	/// Broadcast a system notification to ALL connected users.
	pub fn broadcast_system_notification(&self, payload: &SystemNotificationPayload) {
		let msg = WsMessage::SystemNotification(payload.clone());
		let json = match serde_json::to_string(&msg) {
			Ok(j) => j,
			Err(_) => return,
		};

		for entry in self.connections.iter() {
			self.send_to_user(entry.key(), &json);
		}
	}

	/// Remove all connections and subscriptions for a user.
	pub fn cleanup_user(&self, user_id: &str) {
		self.connections.remove(user_id);
		self.cleanup_subscriptions(user_id);
	}

	/// Remove all subscription entries for a user.
	fn cleanup_subscriptions(&self, user_id: &str) {
		if let Some((_, dep_ids)) = self.user_subscriptions.remove(user_id) {
			for dep_id in &dep_ids {
				if let Some(mut users) = self.subscriptions.get_mut(dep_id) {
					users.remove(user_id);
					if users.is_empty() {
						drop(users);
						self.subscriptions.remove(dep_id);
					}
				}
			}
		}
	}

	/// Send a pre-serialized JSON string to all connections of a user.
	fn send_to_user(&self, user_id: &str, json: &str) {
		if let Some(conns) = self.connections.get(user_id) {
			for conn in conns.iter() {
				// Ignore send errors — the connection will be cleaned up
				// by the read-loop when it detects the closed channel.
				let _ = conn.sender.send(json.to_string());
			}
		}
	}
}

impl Default for WsBroadcaster {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	use tokio::sync::mpsc;

	use crate::shared::ws_messages::{DeploymentState, NotificationLevel};

	/// Helper: create a `ConnectionHandle` and return it alongside the receiver.
	fn make_connection(
		user_id: &str,
		connection_id: &str,
	) -> (ConnectionHandle, mpsc::UnboundedReceiver<String>) {
		let (tx, rx) = mpsc::unbounded_channel();
		let handle = ConnectionHandle {
			connection_id: connection_id.to_string(),
			user_id: user_id.to_string(),
			sender: tx,
		};
		(handle, rx)
	}

	#[rstest]
	fn test_register_and_remove_connection() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let (handle, _rx) = make_connection("user-1", "conn-1");

		// Act — register
		broadcaster.register_connection(handle);

		// Assert — one connected user
		assert_eq!(broadcaster.connection_count(), 1);

		// Act — remove
		broadcaster.remove_connection("user-1", "conn-1");

		// Assert — no connected users
		assert_eq!(broadcaster.connection_count(), 0);
	}

	#[rstest]
	fn test_subscribe_and_unsubscribe() {
		// Arrange
		let broadcaster = WsBroadcaster::new();

		// Act — subscribe to two deployments
		broadcaster.subscribe("user-1", "dep-a");
		broadcaster.subscribe("user-1", "dep-b");

		// Assert — both subscribed
		assert!(broadcaster.is_subscribed("user-1", "dep-a"));
		assert!(broadcaster.is_subscribed("user-1", "dep-b"));

		// Act — unsubscribe from one
		broadcaster.unsubscribe("user-1", "dep-a");

		// Assert — only dep-b remains
		assert!(!broadcaster.is_subscribed("user-1", "dep-a"));
		assert!(broadcaster.is_subscribed("user-1", "dep-b"));
	}

	#[rstest]
	fn test_subscription_limit_enforced() {
		// Arrange
		let broadcaster = WsBroadcaster::new();

		// Act — subscribe to the maximum allowed
		for i in 0..MAX_SUBSCRIPTIONS_PER_USER {
			let result = broadcaster.try_subscribe("user-1", &format!("dep-{i}"));
			assert!(result, "subscription {i} should succeed");
		}

		// Act — attempt one more beyond the limit
		let over_limit = broadcaster.try_subscribe("user-1", "dep-overflow");

		// Assert — rejected
		assert!(!over_limit);
		assert!(!broadcaster.is_subscribed("user-1", "dep-overflow"));
	}

	#[rstest]
	fn test_broadcast_deployment_status_reaches_subscribers_only() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let (h1, mut rx1) = make_connection("user-1", "conn-1");
		let (h2, mut rx2) = make_connection("user-2", "conn-2");
		broadcaster.register_connection(h1);
		broadcaster.register_connection(h2);

		// Only user-1 subscribes to dep-a
		broadcaster.subscribe("user-1", "dep-a");

		let payload = DeploymentStatusPayload {
			deployment_id: "dep-a".to_string(),
			name: "my-app".to_string(),
			namespace: "default".to_string(),
			status: DeploymentState::Running,
			ready_replicas: 2,
			desired_replicas: 2,
			message: None,
			timestamp: "2026-03-22T00:00:00Z".to_string(),
		};

		// Act
		broadcaster.broadcast_deployment_status(&payload);

		// Assert — user-1 received the message
		let msg1 = rx1.try_recv();
		assert!(msg1.is_ok(), "user-1 should receive the message");

		let parsed: serde_json::Value = serde_json::from_str(&msg1.unwrap()).unwrap();
		assert_eq!(parsed["type"], "DeploymentStatus");
		assert_eq!(parsed["payload"]["deployment_id"], "dep-a");

		// Assert — user-2 did NOT receive anything
		let msg2 = rx2.try_recv();
		assert!(msg2.is_err(), "user-2 should NOT receive the message");
	}

	#[rstest]
	fn test_broadcast_system_notification_reaches_all() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let (h1, mut rx1) = make_connection("user-1", "conn-1");
		let (h2, mut rx2) = make_connection("user-2", "conn-2");
		broadcaster.register_connection(h1);
		broadcaster.register_connection(h2);

		let payload = SystemNotificationPayload {
			id: "notif-1".to_string(),
			level: NotificationLevel::Info,
			title: "Maintenance".to_string(),
			message: "Scheduled maintenance at 02:00 UTC".to_string(),
			timestamp: "2026-03-22T00:00:00Z".to_string(),
		};

		// Act
		broadcaster.broadcast_system_notification(&payload);

		// Assert — both users received the notification
		let msg1 = rx1.try_recv();
		assert!(msg1.is_ok(), "user-1 should receive the notification");

		let msg2 = rx2.try_recv();
		assert!(msg2.is_ok(), "user-2 should receive the notification");

		// Verify content for one of them
		let parsed: serde_json::Value = serde_json::from_str(&msg1.unwrap()).unwrap();
		assert_eq!(parsed["type"], "SystemNotification");
		assert_eq!(parsed["payload"]["title"], "Maintenance");
	}

	#[rstest]
	fn test_cleanup_user_removes_connections_and_subscriptions() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let (handle, _rx) = make_connection("user-1", "conn-1");
		broadcaster.register_connection(handle);
		broadcaster.subscribe("user-1", "dep-a");
		broadcaster.subscribe("user-1", "dep-b");

		// Verify preconditions
		assert_eq!(broadcaster.connection_count(), 1);
		assert!(broadcaster.is_subscribed("user-1", "dep-a"));

		// Act
		broadcaster.cleanup_user("user-1");

		// Assert — both connections and subscriptions are removed
		assert_eq!(broadcaster.connection_count(), 0);
		assert!(!broadcaster.is_subscribed("user-1", "dep-a"));
		assert!(!broadcaster.is_subscribed("user-1", "dep-b"));
	}
}
