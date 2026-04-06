//! WebSocket consumer for real-time notification delivery.
//!
//! `NotificationConsumer` implements the `WebSocketConsumer` trait from
//! reinhardt-websockets, bridging incoming WebSocket connections to the
//! `WsBroadcaster` event distribution system.

use std::sync::Arc;

use reinhardt::{ConsumerContext, Message, WebSocketConsumer, WebSocketResult};
use uuid::Uuid;

use crate::apps::auth::services::session::validate_session;
use crate::apps::realtime::broadcaster::WsBroadcaster;
use crate::shared::ws_messages::{
	LogStreamAckPayload, NotificationLevel, SystemNotificationPayload, WsClientMessage, WsMessage,
};

/// Metadata key for the connection UUID assigned during `on_connect`.
const META_CONNECTION_ID: &str = "connection_id";

/// Metadata key for the authenticated user ID (set after successful auth).
const META_USER_ID: &str = "user_id";

/// Parsed result from a client message before async execution.
///
/// Separates the synchronous parsing/validation phase from the async
/// broadcaster interaction to keep unit tests simple.
pub(crate) enum ParsedAction {
	/// Subscribe to deployments (requires auth).
	Subscribe { deployment_ids: Vec<String> },
	/// Unsubscribe from deployments.
	Unsubscribe { deployment_ids: Vec<String> },
	/// Unauthenticated request attempt — send error response.
	Rejected { response: WsMessage },
	/// Log stream subscription acknowledged — send ack and delegate to gRPC bridge.
	LogStreamAcknowledged { response: WsMessage },
}

/// WebSocket consumer that authenticates users, manages deployment
/// subscriptions, and forwards broadcaster events to individual connections.
///
/// Unlike the previous `ConnectionHandle`-based approach, connections are
/// registered directly with the [`WsBroadcaster`] rooms. This eliminates
/// the per-connection mpsc channel and forwarding task — the Room broadcasts
/// to `Arc<WebSocketConnection>` instances directly.
pub struct NotificationConsumer {
	broadcaster: Arc<WsBroadcaster>,
}

impl NotificationConsumer {
	/// Create a new consumer backed by the given broadcaster.
	pub fn new(broadcaster: Arc<WsBroadcaster>) -> Self {
		Self { broadcaster }
	}

	/// Parse a client message and validate authentication, returning a
	/// [`ParsedAction`] that describes what async work to perform.
	///
	/// This is a synchronous function for easy unit testing.
	pub(crate) fn parse_client_message(
		user_id: Option<&str>,
		msg: WsClientMessage,
	) -> ParsedAction {
		match msg {
			WsClientMessage::Subscribe { deployment_ids } => {
				if user_id.is_none() {
					return ParsedAction::Rejected {
						response: WsMessage::SystemNotification(SystemNotificationPayload {
							id: uuid::Uuid::new_v4().to_string(),
							level: NotificationLevel::Critical,
							title: "Authentication required".to_string(),
							message: "You must be authenticated to subscribe".to_string(),
							timestamp: String::new(),
						}),
					};
				}
				ParsedAction::Subscribe { deployment_ids }
			}
			WsClientMessage::Unsubscribe { deployment_ids } => {
				ParsedAction::Unsubscribe { deployment_ids }
			}
			// Log streaming subscriptions — acknowledged but not yet wired to the
			// gRPC log bridge. When the bridge is implemented, the
			// LogStreamAcknowledged handler should start/stop the stream.
			WsClientMessage::SubscribeBuildLogs { .. }
			| WsClientMessage::SubscribeAppLogs { .. }
			| WsClientMessage::UnsubscribeLogs => {
				if user_id.is_none() {
					return ParsedAction::Rejected {
						response: WsMessage::SystemNotification(SystemNotificationPayload {
							id: uuid::Uuid::new_v4().to_string(),
							level: NotificationLevel::Critical,
							title: "Authentication required".to_string(),
							message: "You must be authenticated to subscribe".to_string(),
							timestamp: String::new(),
						}),
					};
				}
				ParsedAction::LogStreamAcknowledged {
					response: WsMessage::LogStreamAck(LogStreamAckPayload {
						acknowledged: true,
						message: "Log streaming subscription acknowledged".to_string(),
					}),
				}
			}
		}
	}
}

/// Extract a named cookie value from a `Cookie` header string.
fn extract_cookie_value(cookie_header: &str, name: &str) -> Option<String> {
	cookie_header.split(';').find_map(|pair| {
		let (key, value) = pair.trim().split_once('=')?;
		if key.trim() == name {
			Some(value.trim().to_string())
		} else {
			None
		}
	})
}

#[async_trait::async_trait]
impl WebSocketConsumer for NotificationConsumer {
	async fn on_connect(&self, context: &mut ConsumerContext) -> WebSocketResult<()> {
		let connection_id = Uuid::new_v4().to_string();
		context
			.metadata
			.insert(META_CONNECTION_ID.to_string(), connection_id.clone());

		// Authenticate from session cookie in handshake headers
		if let Some(cookie_header) = context.cookie_header() {
			if let Some(session_id) = extract_cookie_value(cookie_header, "sessionid") {
				if let Some((user_id, _username)) = validate_session(&session_id).await {
					context
						.metadata
						.insert(META_USER_ID.to_string(), user_id.clone());
					self.broadcaster
						.register_connection(
							&connection_id,
							&user_id,
							Arc::clone(&context.connection),
						)
						.await;
				}
			}
		}

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

		let action = Self::parse_client_message(user_id.as_deref(), client_msg);

		match action {
			ParsedAction::Rejected { response }
			| ParsedAction::LogStreamAcknowledged { response } => {
				let _ = context.connection.send_json(&response).await;
			}
			ParsedAction::Subscribe { deployment_ids } => {
				if let Some(uid) = user_id {
					for dep_id in &deployment_ids {
						self.broadcaster
							.try_subscribe(&connection_id, &uid, dep_id)
							.await;
					}
				}
			}
			ParsedAction::Unsubscribe { deployment_ids } => {
				for dep_id in &deployment_ids {
					self.broadcaster.unsubscribe(&connection_id, dep_id).await;
				}
			}
		}

		Ok(())
	}

	async fn on_disconnect(&self, context: &mut ConsumerContext) -> WebSocketResult<()> {
		let connection_id = context
			.get_metadata(META_CONNECTION_ID)
			.map_or(String::new(), |v| v.to_string());

		self.broadcaster.remove_connection(&connection_id).await;

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::shared::ws_messages::WsClientMessage;
	use rstest::rstest;

	#[rstest]
	fn test_parse_subscribe_without_auth_rejected() {
		// Arrange
		let msg = WsClientMessage::Subscribe {
			deployment_ids: vec!["dep-1".to_string()],
		};

		// Act — no user_id (unauthenticated)
		let action = NotificationConsumer::parse_client_message(None, msg);

		// Assert
		match action {
			ParsedAction::Rejected { response } => match &response {
				WsMessage::SystemNotification(payload) => {
					assert_eq!(payload.level, NotificationLevel::Critical);
					assert_eq!(payload.title, "Authentication required");
				}
				_ => panic!("expected SystemNotification rejection"),
			},
			_ => panic!("expected Rejected action"),
		}
	}

	#[rstest]
	fn test_parse_subscribe_with_auth_returns_subscribe_action() {
		// Arrange
		let msg = WsClientMessage::Subscribe {
			deployment_ids: vec!["dep-1".to_string(), "dep-2".to_string()],
		};

		// Act — authenticated user subscribes
		let action = NotificationConsumer::parse_client_message(Some("user-1"), msg);

		// Assert — returns Subscribe action
		match action {
			ParsedAction::Subscribe { deployment_ids } => {
				assert_eq!(deployment_ids, vec!["dep-1", "dep-2"]);
			}
			_ => panic!("expected Subscribe action"),
		}
	}

	#[rstest]
	fn test_parse_unsubscribe_returns_unsubscribe_action() {
		// Arrange
		let msg = WsClientMessage::Unsubscribe {
			deployment_ids: vec!["dep-a".to_string()],
		};

		// Act
		let action = NotificationConsumer::parse_client_message(Some("user-1"), msg);

		// Assert
		match action {
			ParsedAction::Unsubscribe { deployment_ids } => {
				assert_eq!(deployment_ids, vec!["dep-a"]);
			}
			_ => panic!("expected Unsubscribe action"),
		}
	}

	#[rstest]
	fn test_parse_unsubscribe_without_auth_works() {
		// Arrange
		let msg = WsClientMessage::Unsubscribe {
			deployment_ids: vec!["dep-1".to_string()],
		};

		// Act — no user_id (unauthenticated), but Unsubscribe should still work
		let action = NotificationConsumer::parse_client_message(None, msg);

		// Assert
		match action {
			ParsedAction::Unsubscribe { deployment_ids } => {
				assert_eq!(deployment_ids, vec!["dep-1"]);
			}
			_ => panic!("expected Unsubscribe action"),
		}
	}

	#[rstest]
	fn test_parse_subscribe_empty_deployment_ids() {
		// Arrange
		let msg = WsClientMessage::Subscribe {
			deployment_ids: vec![],
		};

		// Act — authenticated user subscribes with empty list
		let action = NotificationConsumer::parse_client_message(Some("user-1"), msg);

		// Assert
		match action {
			ParsedAction::Subscribe { deployment_ids } => {
				assert!(deployment_ids.is_empty());
			}
			_ => panic!("expected Subscribe action"),
		}
	}

	#[rstest]
	fn test_extract_cookie_value_single() {
		assert_eq!(
			extract_cookie_value("sessionid=abc123", "sessionid"),
			Some("abc123".to_string())
		);
	}

	#[rstest]
	fn test_extract_cookie_value_multiple() {
		assert_eq!(
			extract_cookie_value("csrftoken=xyz; sessionid=abc123; other=val", "sessionid"),
			Some("abc123".to_string())
		);
	}

	#[rstest]
	fn test_extract_cookie_value_missing() {
		assert_eq!(
			extract_cookie_value("csrftoken=xyz; other=val", "sessionid"),
			None
		);
	}

	#[rstest]
	#[tokio::test]
	async fn test_on_disconnect_cleans_up_broadcaster() {
		// Arrange
		let broadcaster = Arc::new(WsBroadcaster::new());

		let (conn, _rx) = {
			let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
			let conn = Arc::new(reinhardt::WebSocketConnection::new(
				"conn-1".to_string(),
				tx,
			));
			(conn, rx)
		};

		broadcaster
			.register_connection("conn-1", "user-1", conn.clone())
			.await;
		broadcaster.subscribe("conn-1", "user-1", "dep-x").await;
		assert_eq!(broadcaster.connection_count().await, 1);

		// Act — simulate disconnect via broadcaster directly
		broadcaster.remove_connection("conn-1").await;

		// Assert — connection removed; since it was the last connection,
		// subscriptions are also cleaned up.
		assert_eq!(broadcaster.connection_count().await, 0);
		assert!(!broadcaster.is_subscribed("user-1", "dep-x").await);
	}
}
