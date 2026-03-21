//! WebSocket consumer for real-time notification delivery.
//!
//! `NotificationConsumer` implements the `WebSocketConsumer` trait from
//! reinhardt-websockets, bridging incoming WebSocket connections to the
//! `WsBroadcaster` event distribution system.

use std::sync::Arc;

use reinhardt::{ConsumerContext, Message, WebSocketConsumer, WebSocketResult};
use uuid::Uuid;

use crate::apps::auth::services::session::validate_raw_token;
use crate::apps::realtime::broadcaster::{ConnectionHandle, WsBroadcaster};
use crate::shared::ws_messages::{AuthResultPayload, WsClientMessage, WsMessage};

/// Metadata key for the connection UUID assigned during `on_connect`.
const META_CONNECTION_ID: &str = "connection_id";

/// Metadata key for the authenticated user ID (set after successful auth).
const META_USER_ID: &str = "user_id";

/// WebSocket consumer that authenticates users, manages deployment
/// subscriptions, and forwards broadcaster events to individual connections.
pub struct NotificationConsumer {
	broadcaster: Arc<WsBroadcaster>,
}

impl NotificationConsumer {
	/// Create a new consumer backed by the given broadcaster.
	pub fn new(broadcaster: Arc<WsBroadcaster>) -> Self {
		Self { broadcaster }
	}

	/// Pure, synchronous message handler that is easy to unit-test.
	///
	/// Returns an optional `(response, new_user_id)` tuple:
	/// - `response` is always sent back to the client.
	/// - `new_user_id` is `Some` only when authentication succeeds,
	///   signalling the caller to register the connection.
	pub fn handle_client_message(
		&self,
		user_id: Option<&str>,
		_connection_id: &str,
		msg: WsClientMessage,
	) -> Option<(WsMessage, Option<String>)> {
		match msg {
			WsClientMessage::Authenticate { token } => {
				match validate_raw_token(&token) {
					Some((uid, _username)) => {
						let response = WsMessage::AuthResult(AuthResultPayload {
							success: true,
							message: None,
						});
						Some((response, Some(uid)))
					}
					None => {
						let response = WsMessage::AuthResult(AuthResultPayload {
							success: false,
							message: Some("Invalid or expired token".to_string()),
						});
						Some((response, None))
					}
				}
			}
			WsClientMessage::Subscribe { deployment_ids } => {
				let Some(uid) = user_id else {
					let response = WsMessage::AuthResult(AuthResultPayload {
						success: false,
						message: Some("Authentication required".to_string()),
					});
					return Some((response, None));
				};

				for dep_id in &deployment_ids {
					self.broadcaster.try_subscribe(uid, dep_id);
				}
				// No explicit response for subscribe — updates arrive via
				// the broadcaster forwarding task.
				None
			}
			WsClientMessage::Unsubscribe { deployment_ids } => {
				if let Some(uid) = user_id {
					for dep_id in &deployment_ids {
						self.broadcaster.unsubscribe(uid, dep_id);
					}
				}
				None
			}
		}
	}

	/// Clean up broadcaster state when a user disconnects.
	pub fn on_user_disconnect(&self, user_id: &str, connection_id: &str) {
		self.broadcaster.remove_connection(user_id, connection_id);
	}
}

#[async_trait::async_trait]
impl WebSocketConsumer for NotificationConsumer {
	async fn on_connect(&self, context: &mut ConsumerContext) -> WebSocketResult<()> {
		let connection_id = Uuid::new_v4().to_string();
		context
			.metadata
			.insert(META_CONNECTION_ID.to_string(), connection_id);
		Ok(())
	}

	async fn on_message(
		&self,
		context: &mut ConsumerContext,
		message: Message,
	) -> WebSocketResult<()> {
		let text = match &message {
			Message::Text { data } => data.clone(),
			// Ignore non-text frames (ping/pong handled by the framework).
			_ => return Ok(()),
		};

		let client_msg: WsClientMessage = match serde_json::from_str(&text) {
			Ok(m) => m,
			Err(_) => return Ok(()),
		};

		let user_id = context.get_metadata(META_USER_ID).map(|s| s.to_string());
		let connection_id = context
			.get_metadata(META_CONNECTION_ID)
			.map_or(String::new(), |v| v.to_string());

		let result =
			self.handle_client_message(user_id.as_deref(), &connection_id, client_msg);

		if let Some((response, maybe_new_uid)) = result {
			// Send the response message to the client.
			let _ = context.connection.send_json(&response).await;

			// On successful authentication, register the connection with the
			// broadcaster and spawn a forwarding task that relays broadcaster
			// events to this specific WebSocket connection.
			if let Some(ref uid) = maybe_new_uid {
				context
					.metadata
					.insert(META_USER_ID.to_string(), uid.clone());

				let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
				let handle = ConnectionHandle {
					connection_id: connection_id.clone(),
					user_id: uid.clone(),
					sender: tx,
				};
				self.broadcaster.register_connection(handle);

				// Forward messages from the broadcaster channel to the
				// WebSocket connection.
				let conn = Arc::clone(&context.connection);
				tokio::spawn(async move {
					while let Some(json) = rx.recv().await {
						if conn.send_text(json).await.is_err() {
							break;
						}
					}
				});
			}
		}

		Ok(())
	}

	async fn on_disconnect(&self, context: &mut ConsumerContext) -> WebSocketResult<()> {
		let user_id = context.get_metadata(META_USER_ID).map(|s| s.to_string());
		let connection_id = context
			.get_metadata(META_CONNECTION_ID)
			.map_or(String::new(), |v| v.to_string());

		if let Some(uid) = user_id {
			self.on_user_disconnect(&uid, &connection_id);
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::ws_messages::WsClientMessage;
	use rstest::rstest;
	use serial_test::serial;
	use std::sync::Arc;

	fn test_broadcaster() -> Arc<WsBroadcaster> {
		Arc::new(WsBroadcaster::new())
	}

	fn setup_jwt_env() {
		// SAFETY: Tests using this helper are serialized with #[serial(jwt)]
		// to prevent concurrent environment variable mutation.
		unsafe {
			std::env::set_var(
				"REINHARDT_CLOUD_JWT_SECRET",
				"test-secret-key-for-unit-tests-minimum-length-32",
			);
		}
	}

	#[rstest]
	#[serial(jwt)]
	fn test_handle_authenticate_with_invalid_token_returns_failure() {
		// Arrange
		setup_jwt_env();
		let broadcaster = test_broadcaster();
		let consumer = NotificationConsumer::new(broadcaster);

		let msg = WsClientMessage::Authenticate {
			token: "totally-invalid-jwt-token".to_string(),
		};

		// Act
		let result = consumer.handle_client_message(None, "conn-1", msg);

		// Assert
		let (response, new_uid) = result.expect("should return a response");
		match &response {
			WsMessage::AuthResult(payload) => {
				assert!(!payload.success);
				assert!(payload.message.is_some());
			}
			_ => panic!("expected AuthResult"),
		}
		assert!(new_uid.is_none(), "invalid token should not yield a user id");
	}

	#[rstest]
	fn test_handle_subscribe_without_auth_rejected() {
		// Arrange
		let broadcaster = test_broadcaster();
		let consumer = NotificationConsumer::new(broadcaster);

		let msg = WsClientMessage::Subscribe {
			deployment_ids: vec!["dep-1".to_string()],
		};

		// Act — no user_id (unauthenticated)
		let result = consumer.handle_client_message(None, "conn-1", msg);

		// Assert
		let (response, new_uid) = result.expect("should return error response");
		match &response {
			WsMessage::AuthResult(payload) => {
				assert!(!payload.success);
				assert_eq!(
					payload.message.as_deref(),
					Some("Authentication required")
				);
			}
			_ => panic!("expected AuthResult rejection"),
		}
		assert!(new_uid.is_none());
	}

	#[rstest]
	fn test_handle_subscribe_with_auth_succeeds() {
		// Arrange
		let broadcaster = test_broadcaster();
		let consumer = NotificationConsumer::new(Arc::clone(&broadcaster));

		// Act — authenticated user subscribes
		let msg = WsClientMessage::Subscribe {
			deployment_ids: vec!["dep-1".to_string(), "dep-2".to_string()],
		};
		let result = consumer.handle_client_message(Some("user-1"), "conn-1", msg);

		// Assert — no response message (subscriptions are silent)
		assert!(result.is_none(), "subscribe should not return a response");
		assert!(broadcaster.is_subscribed("user-1", "dep-1"));
		assert!(broadcaster.is_subscribed("user-1", "dep-2"));
	}

	#[rstest]
	fn test_handle_unsubscribe_removes_subscription() {
		// Arrange
		let broadcaster = test_broadcaster();
		let consumer = NotificationConsumer::new(Arc::clone(&broadcaster));

		// Pre-subscribe
		broadcaster.subscribe("user-1", "dep-a");
		broadcaster.subscribe("user-1", "dep-b");
		assert!(broadcaster.is_subscribed("user-1", "dep-a"));

		// Act
		let msg = WsClientMessage::Unsubscribe {
			deployment_ids: vec!["dep-a".to_string()],
		};
		let result = consumer.handle_client_message(Some("user-1"), "conn-1", msg);

		// Assert
		assert!(result.is_none(), "unsubscribe should not return a response");
		assert!(!broadcaster.is_subscribed("user-1", "dep-a"));
		assert!(broadcaster.is_subscribed("user-1", "dep-b"));
	}

	#[rstest]
	fn test_on_user_disconnect_cleans_up() {
		// Arrange
		let broadcaster = test_broadcaster();
		let consumer = NotificationConsumer::new(Arc::clone(&broadcaster));

		let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
		let handle = ConnectionHandle {
			connection_id: "conn-1".to_string(),
			user_id: "user-1".to_string(),
			sender: tx,
		};
		broadcaster.register_connection(handle);
		broadcaster.subscribe("user-1", "dep-x");
		assert_eq!(broadcaster.connection_count(), 1);

		// Act
		consumer.on_user_disconnect("user-1", "conn-1");

		// Assert — connection removed; since it was the last connection,
		// subscriptions are also cleaned up.
		assert_eq!(broadcaster.connection_count(), 0);
		assert!(!broadcaster.is_subscribed("user-1", "dep-x"));
	}
}
