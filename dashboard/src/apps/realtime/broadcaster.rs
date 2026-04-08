//! WebSocket event broadcaster for distributing real-time updates.
//!
//! `WsBroadcaster` wraps reinhardt-websocket's [`RoomManager`] to manage
//! deployment subscriptions and system-wide notifications. Each deployment
//! maps to a dedicated [`Room`], and a special `"system:all"` room delivers
//! messages to every authenticated connection.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use reinhardt::di::injectable_factory;
use reinhardt::{Message, RoomManager, WebSocketConnection};
use tokio::sync::RwLock;

use crate::shared::ws_messages::{DeploymentStatusPayload, SystemNotificationPayload, WsMessage};

/// Maximum number of deployment subscriptions allowed per user.
pub const MAX_SUBSCRIPTIONS_PER_USER: usize = 100;

/// Room ID used for system-wide notifications delivered to all connections.
const SYSTEM_ROOM_ID: &str = "system:all";

/// Server-side broadcaster that fans out WebSocket messages to connected clients.
///
/// # Design
///
/// - **room_manager**: Manages per-deployment rooms and the system notification room.
/// - **user_subscriptions**: Tracks which deployments each user subscribes to
///   (for enforcing [`MAX_SUBSCRIPTIONS_PER_USER`]).
/// - **connections**: Stores `Arc<WebSocketConnection>` by connection_id so they
///   can be joined to deployment rooms on subscribe.
/// - **connection_users**: Reverse mapping from connection_id to user_id for cleanup.
/// - **user_connections**: Tracks all connection_ids belonging to a user.
/// - **connection_rooms**: Tracks which deployment rooms each connection has joined.
pub struct WsBroadcaster {
	room_manager: RoomManager,
	/// user_id -> set of deployment_ids the user is subscribed to
	user_subscriptions: RwLock<HashMap<String, HashSet<String>>>,
	/// connection_id -> `Arc<WebSocketConnection>`
	connections: RwLock<HashMap<String, Arc<WebSocketConnection>>>,
	/// connection_id -> user_id
	connection_users: RwLock<HashMap<String, String>>,
	/// user_id -> set of connection_ids
	user_connections: RwLock<HashMap<String, HashSet<String>>>,
	/// connection_id -> set of deployment room_ids
	connection_rooms: RwLock<HashMap<String, HashSet<String>>>,
}

impl WsBroadcaster {
	/// Create a new empty broadcaster.
	pub fn new() -> Self {
		Self {
			room_manager: RoomManager::new(),
			user_subscriptions: RwLock::new(HashMap::new()),
			connections: RwLock::new(HashMap::new()),
			connection_users: RwLock::new(HashMap::new()),
			user_connections: RwLock::new(HashMap::new()),
			connection_rooms: RwLock::new(HashMap::new()),
		}
	}

	/// Register a new WebSocket connection for a user.
	///
	/// The connection is added to the system notification room so it
	/// receives all system-wide broadcasts.
	pub async fn register_connection(
		&self,
		connection_id: &str,
		user_id: &str,
		connection: Arc<WebSocketConnection>,
	) {
		// Store the connection reference for later use in subscribe
		self.connections
			.write()
			.await
			.insert(connection_id.to_string(), Arc::clone(&connection));

		// Join the system notification room
		let system_room = self
			.room_manager
			.get_or_create_room(SYSTEM_ROOM_ID.to_string())
			.await;
		let _ = system_room
			.join(connection_id.to_string(), connection)
			.await;

		// Track connection -> user mapping
		self.connection_users
			.write()
			.await
			.insert(connection_id.to_string(), user_id.to_string());

		// Track user -> connections mapping
		self.user_connections
			.write()
			.await
			.entry(user_id.to_string())
			.or_default()
			.insert(connection_id.to_string());
	}

	/// Remove a specific connection. If this was the user's last connection,
	/// their subscriptions are also cleaned up.
	pub async fn remove_connection(&self, connection_id: &str) {
		// Remove stored connection reference
		self.connections.write().await.remove(connection_id);

		// Look up the user for this connection
		let user_id = self.connection_users.write().await.remove(connection_id);

		// Leave all deployment rooms this connection was in
		if let Some(room_ids) = self.connection_rooms.write().await.remove(connection_id) {
			for room_id in &room_ids {
				if let Some(room) = self.room_manager.get_room(room_id).await {
					let _ = room.leave(connection_id).await;
				}
			}
		}

		// Leave system room
		if let Some(system_room) = self.room_manager.get_room(SYSTEM_ROOM_ID).await {
			let _ = system_room.leave(connection_id).await;
		}

		// If last connection for the user, clean up subscriptions
		if let Some(ref uid) = user_id {
			let should_cleanup = {
				let mut user_conns = self.user_connections.write().await;
				if let Some(conns) = user_conns.get_mut(uid.as_str()) {
					conns.remove(connection_id);
					if conns.is_empty() {
						user_conns.remove(uid.as_str());
						true
					} else {
						false
					}
				} else {
					false
				}
			};

			if should_cleanup {
				self.user_subscriptions.write().await.remove(uid.as_str());
			}
		}

		self.room_manager.cleanup_empty_rooms().await;
	}

	/// Subscribe a user's connection to deployment status updates.
	///
	/// The connection is added to the deployment's room. If the connection
	/// is not tracked, this is a no-op.
	pub async fn subscribe(&self, connection_id: &str, user_id: &str, deployment_id: &str) {
		let room_id = format!("deployment:{deployment_id}");
		let room = self.room_manager.get_or_create_room(room_id.clone()).await;

		// Get the stored connection
		if let Some(connection) = self.connections.read().await.get(connection_id).cloned() {
			let _ = room.join(connection_id.to_string(), connection).await;

			// Track which rooms this connection has joined
			self.connection_rooms
				.write()
				.await
				.entry(connection_id.to_string())
				.or_default()
				.insert(room_id);
		}

		// Track user-level subscription
		self.user_subscriptions
			.write()
			.await
			.entry(user_id.to_string())
			.or_default()
			.insert(deployment_id.to_string());
	}

	/// Subscribe with per-user limit enforcement. Returns `false` if the
	/// limit would be exceeded.
	pub async fn try_subscribe(
		&self,
		connection_id: &str,
		user_id: &str,
		deployment_id: &str,
	) -> bool {
		// Check if already subscribed (does not count against limit)
		let subs = self.user_subscriptions.read().await;
		if let Some(user_subs) = subs.get(user_id) {
			if user_subs.contains(deployment_id) {
				return true;
			}
			if user_subs.len() >= MAX_SUBSCRIPTIONS_PER_USER {
				return false;
			}
		}
		drop(subs);

		self.subscribe(connection_id, user_id, deployment_id).await;
		true
	}

	/// Unsubscribe a connection from a specific deployment.
	pub async fn unsubscribe(&self, connection_id: &str, deployment_id: &str) {
		let room_id = format!("deployment:{deployment_id}");

		if let Some(room) = self.room_manager.get_room(&room_id).await {
			let _ = room.leave(connection_id).await;
		}

		// Remove from connection's room tracking
		if let Some(rooms) = self.connection_rooms.write().await.get_mut(connection_id) {
			rooms.remove(&room_id);
		}

		// Remove from user-level subscription tracking
		let user_id = self
			.connection_users
			.read()
			.await
			.get(connection_id)
			.cloned();
		if let Some(uid) = user_id {
			let mut subs = self.user_subscriptions.write().await;
			if let Some(user_subs) = subs.get_mut(&uid) {
				user_subs.remove(deployment_id);
				if user_subs.is_empty() {
					subs.remove(&uid);
				}
			}
		}

		self.room_manager.cleanup_empty_rooms().await;
	}

	/// Check whether a user is subscribed to a deployment.
	pub async fn is_subscribed(&self, user_id: &str, deployment_id: &str) -> bool {
		self.user_subscriptions
			.read()
			.await
			.get(user_id)
			.map(|subs| subs.contains(deployment_id))
			.unwrap_or(false)
	}

	/// Total number of connected users (not individual connections).
	pub async fn connection_count(&self) -> usize {
		self.user_connections.read().await.len()
	}

	/// Broadcast a deployment status update to all connections subscribed
	/// to that deployment.
	pub async fn broadcast_deployment_status(&self, payload: &DeploymentStatusPayload) {
		let msg = WsMessage::DeploymentStatus(payload.clone());
		let json = match serde_json::to_string(&msg) {
			Ok(j) => j,
			Err(_) => return,
		};

		let room_id = format!("deployment:{}", payload.deployment_id);
		if let Some(room) = self.room_manager.get_room(&room_id).await {
			room.broadcast(Message::text(json)).await;
		}
	}

	/// Broadcast a system notification to ALL connected users.
	pub async fn broadcast_system_notification(&self, payload: &SystemNotificationPayload) {
		let msg = WsMessage::SystemNotification(payload.clone());
		let json = match serde_json::to_string(&msg) {
			Ok(j) => j,
			Err(_) => return,
		};

		if let Some(room) = self.room_manager.get_room(SYSTEM_ROOM_ID).await {
			room.broadcast(Message::text(json)).await;
		}
	}

	/// Remove all connections and subscriptions for a user.
	pub async fn cleanup_user(&self, user_id: &str) {
		// Get all connection IDs for this user
		let conn_ids: Vec<String> = self
			.user_connections
			.read()
			.await
			.get(user_id)
			.map(|ids| ids.iter().cloned().collect())
			.unwrap_or_default();

		for conn_id in &conn_ids {
			// Remove stored connection reference
			self.connections.write().await.remove(conn_id.as_str());

			// Leave all deployment rooms
			if let Some(room_ids) = self.connection_rooms.write().await.remove(conn_id.as_str()) {
				for room_id in &room_ids {
					if let Some(room) = self.room_manager.get_room(room_id).await {
						let _ = room.leave(conn_id).await;
					}
				}
			}

			// Leave system room
			if let Some(system_room) = self.room_manager.get_room(SYSTEM_ROOM_ID).await {
				let _ = system_room.leave(conn_id).await;
			}

			// Remove connection -> user mapping
			self.connection_users.write().await.remove(conn_id.as_str());
		}

		// Remove user-level tracking
		self.user_connections.write().await.remove(user_id);
		self.user_subscriptions.write().await.remove(user_id);

		self.room_manager.cleanup_empty_rooms().await;
	}
}

impl Default for WsBroadcaster {
	fn default() -> Self {
		Self::new()
	}
}

/// DI factory — auto-registers `WsBroadcaster` as a singleton.
/// Tests can override via `SingletonScope::set()` before resolution.
#[injectable_factory(scope = "singleton")]
async fn create_ws_broadcaster() -> WsBroadcaster {
	WsBroadcaster::new()
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	use tokio::sync::mpsc;

	use crate::shared::ws_messages::{DeploymentState, NotificationLevel};

	/// Helper: create a `WebSocketConnection` and return it alongside the receiver.
	fn make_connection(
		connection_id: &str,
	) -> (Arc<WebSocketConnection>, mpsc::UnboundedReceiver<Message>) {
		let (tx, rx) = mpsc::unbounded_channel();
		let conn = Arc::new(WebSocketConnection::new(connection_id.to_string(), tx));
		(conn, rx)
	}

	#[rstest]
	#[tokio::test]
	async fn test_register_and_remove_connection() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let (conn, _rx) = make_connection("conn-1");

		// Act — register
		broadcaster
			.register_connection("conn-1", "user-1", conn)
			.await;

		// Assert — one connected user
		assert_eq!(broadcaster.connection_count().await, 1);

		// Act — remove
		broadcaster.remove_connection("conn-1").await;

		// Assert — no connected users
		assert_eq!(broadcaster.connection_count().await, 0);
	}

	#[rstest]
	#[tokio::test]
	async fn test_subscribe_and_unsubscribe() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let (conn, _rx) = make_connection("conn-1");
		broadcaster
			.register_connection("conn-1", "user-1", conn)
			.await;

		// Act — subscribe to two deployments
		broadcaster.subscribe("conn-1", "user-1", "dep-a").await;
		broadcaster.subscribe("conn-1", "user-1", "dep-b").await;

		// Assert — both subscribed
		assert!(broadcaster.is_subscribed("user-1", "dep-a").await);
		assert!(broadcaster.is_subscribed("user-1", "dep-b").await);

		// Act — unsubscribe from one
		broadcaster.unsubscribe("conn-1", "dep-a").await;

		// Assert — only dep-b remains
		assert!(!broadcaster.is_subscribed("user-1", "dep-a").await);
		assert!(broadcaster.is_subscribed("user-1", "dep-b").await);
	}

	#[rstest]
	#[tokio::test]
	async fn test_subscription_limit_enforced() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let (conn, _rx) = make_connection("conn-1");
		broadcaster
			.register_connection("conn-1", "user-1", conn)
			.await;

		// Act — subscribe to the maximum allowed
		for i in 0..MAX_SUBSCRIPTIONS_PER_USER {
			let result = broadcaster
				.try_subscribe("conn-1", "user-1", &format!("dep-{i}"))
				.await;
			assert!(result, "subscription {i} should succeed");
		}

		// Act — attempt one more beyond the limit
		let over_limit = broadcaster
			.try_subscribe("conn-1", "user-1", "dep-overflow")
			.await;

		// Assert — rejected
		assert!(!over_limit);
		assert!(!broadcaster.is_subscribed("user-1", "dep-overflow").await);
	}

	#[rstest]
	#[tokio::test]
	async fn test_broadcast_deployment_status_reaches_subscribers_only() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let (conn1, mut rx1) = make_connection("conn-1");
		let (conn2, mut rx2) = make_connection("conn-2");
		broadcaster
			.register_connection("conn-1", "user-1", conn1)
			.await;
		broadcaster
			.register_connection("conn-2", "user-2", conn2)
			.await;

		// Only user-1 subscribes to dep-a
		broadcaster.subscribe("conn-1", "user-1", "dep-a").await;

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
		broadcaster.broadcast_deployment_status(&payload).await;

		// Assert — user-1 received the message
		let msg1 = rx1.try_recv();
		assert!(msg1.is_ok(), "user-1 should receive the message");

		let text = match msg1.unwrap() {
			Message::Text { data } => data,
			other => panic!("expected Text message, got {other:?}"),
		};
		let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
		assert_eq!(parsed["type"], "DeploymentStatus");
		assert_eq!(parsed["payload"]["deployment_id"], "dep-a");

		// Assert — user-2 did NOT receive anything (not subscribed to dep-a)
		let msg2 = rx2.try_recv();
		assert!(msg2.is_err(), "user-2 should NOT receive the message");
	}

	#[rstest]
	#[tokio::test]
	async fn test_broadcast_system_notification_reaches_all() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let (conn1, mut rx1) = make_connection("conn-1");
		let (conn2, mut rx2) = make_connection("conn-2");
		broadcaster
			.register_connection("conn-1", "user-1", conn1)
			.await;
		broadcaster
			.register_connection("conn-2", "user-2", conn2)
			.await;

		let payload = SystemNotificationPayload {
			id: "notif-1".to_string(),
			level: NotificationLevel::Info,
			title: "Maintenance".to_string(),
			message: "Scheduled maintenance at 02:00 UTC".to_string(),
			timestamp: "2026-03-22T00:00:00Z".to_string(),
		};

		// Act
		broadcaster.broadcast_system_notification(&payload).await;

		// Assert — both users received the notification
		let msg1 = rx1.try_recv();
		assert!(msg1.is_ok(), "user-1 should receive the notification");

		let msg2 = rx2.try_recv();
		assert!(msg2.is_ok(), "user-2 should receive the notification");

		// Verify content for one of them
		let text = match msg1.unwrap() {
			Message::Text { data } => data,
			other => panic!("expected Text message, got {other:?}"),
		};
		let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
		assert_eq!(parsed["type"], "SystemNotification");
		assert_eq!(parsed["payload"]["title"], "Maintenance");
	}

	#[rstest]
	#[tokio::test]
	async fn test_multiple_connections_same_user() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let (conn1, mut rx1) = make_connection("conn-1");
		let (conn2, mut rx2) = make_connection("conn-2");

		// Act — register two connections for the same user
		broadcaster
			.register_connection("conn-1", "user-1", conn1)
			.await;
		broadcaster
			.register_connection("conn-2", "user-1", conn2)
			.await;

		// Assert — connection_count counts users, not connections
		assert_eq!(broadcaster.connection_count().await, 1);

		// Act — broadcast system notification
		let payload = SystemNotificationPayload {
			id: "notif-1".to_string(),
			level: NotificationLevel::Info,
			title: "Test".to_string(),
			message: "Broadcast test".to_string(),
			timestamp: "2026-03-22T00:00:00Z".to_string(),
		};
		broadcaster.broadcast_system_notification(&payload).await;

		// Assert — both connections receive the broadcast
		assert!(rx1.try_recv().is_ok(), "conn-1 should receive the message");
		assert!(rx2.try_recv().is_ok(), "conn-2 should receive the message");
	}

	#[rstest]
	#[tokio::test]
	async fn test_remove_one_connection_keeps_subscriptions() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let (conn1, _rx1) = make_connection("conn-1");
		let (conn2, _rx2) = make_connection("conn-2");
		broadcaster
			.register_connection("conn-1", "user-1", conn1)
			.await;
		broadcaster
			.register_connection("conn-2", "user-1", conn2)
			.await;
		broadcaster.subscribe("conn-1", "user-1", "dep-a").await;
		broadcaster.subscribe("conn-2", "user-1", "dep-a").await;

		// Act — remove conn-1 only
		broadcaster.remove_connection("conn-1").await;

		// Assert — user still subscribed because conn-2 is alive
		assert!(broadcaster.is_subscribed("user-1", "dep-a").await);
		assert_eq!(broadcaster.connection_count().await, 1);
	}

	#[rstest]
	#[tokio::test]
	async fn test_try_subscribe_idempotent() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let (conn, _rx) = make_connection("conn-1");
		broadcaster
			.register_connection("conn-1", "user-1", conn)
			.await;

		// Act — subscribe twice to the same deployment
		let first = broadcaster.try_subscribe("conn-1", "user-1", "dep-a").await;
		let second = broadcaster.try_subscribe("conn-1", "user-1", "dep-a").await;

		// Assert — both return true
		assert!(first);
		assert!(second);
		assert!(broadcaster.is_subscribed("user-1", "dep-a").await);
	}

	#[rstest]
	#[tokio::test]
	async fn test_subscribe_unregistered_connection_noop() {
		// Arrange
		let broadcaster = WsBroadcaster::new();

		// Act — subscribe an unknown connection (no panic expected)
		broadcaster.subscribe("ghost-conn", "user-x", "dep-a").await;

		// Assert — user is tracked in subscriptions but connection was a no-op
		assert!(broadcaster.is_subscribed("user-x", "dep-a").await);
	}

	#[rstest]
	#[tokio::test]
	async fn test_cleanup_nonexistent_user_noop() {
		// Arrange
		let broadcaster = WsBroadcaster::new();

		// Act — cleanup a user that was never registered (no panic expected)
		broadcaster.cleanup_user("ghost").await;

		// Assert — still zero connections
		assert_eq!(broadcaster.connection_count().await, 0);
	}

	#[rstest]
	#[tokio::test]
	async fn test_broadcast_to_empty_room_noop() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let payload = DeploymentStatusPayload {
			deployment_id: "dep-nonexistent".to_string(),
			name: "ghost-app".to_string(),
			namespace: "default".to_string(),
			status: DeploymentState::Failed,
			ready_replicas: 0,
			desired_replicas: 1,
			message: None,
			timestamp: "2026-03-22T00:00:00Z".to_string(),
		};

		// Act — broadcast to a deployment with no subscribers (no panic expected)
		broadcaster.broadcast_deployment_status(&payload).await;
	}

	#[rstest]
	#[tokio::test]
	async fn test_connection_full_lifecycle() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let (conn, _rx) = make_connection("conn-1");

		// Act & Assert — register
		broadcaster
			.register_connection("conn-1", "user-1", conn)
			.await;
		assert_eq!(broadcaster.connection_count().await, 1);

		// Act & Assert — subscribe
		broadcaster.subscribe("conn-1", "user-1", "dep-a").await;
		assert!(broadcaster.is_subscribed("user-1", "dep-a").await);

		// Act & Assert — unsubscribe
		broadcaster.unsubscribe("conn-1", "dep-a").await;
		assert!(!broadcaster.is_subscribed("user-1", "dep-a").await);

		// Act & Assert — remove
		broadcaster.remove_connection("conn-1").await;
		assert_eq!(broadcaster.connection_count().await, 0);
	}

	#[rstest]
	#[tokio::test]
	async fn test_cleanup_user_removes_connections_and_subscriptions() {
		// Arrange
		let broadcaster = WsBroadcaster::new();
		let (conn, _rx) = make_connection("conn-1");
		broadcaster
			.register_connection("conn-1", "user-1", conn)
			.await;
		broadcaster.subscribe("conn-1", "user-1", "dep-a").await;
		broadcaster.subscribe("conn-1", "user-1", "dep-b").await;

		// Verify preconditions
		assert_eq!(broadcaster.connection_count().await, 1);
		assert!(broadcaster.is_subscribed("user-1", "dep-a").await);

		// Act
		broadcaster.cleanup_user("user-1").await;

		// Assert — both connections and subscriptions are removed
		assert_eq!(broadcaster.connection_count().await, 0);
		assert!(!broadcaster.is_subscribed("user-1", "dep-a").await);
		assert!(!broadcaster.is_subscribed("user-1", "dep-b").await);
	}
}
