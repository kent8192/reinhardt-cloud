//! WebSocket consumer for real-time notification delivery.
//!
//! `NotificationConsumer` implements the `WebSocketConsumer` trait from
//! reinhardt-websockets, bridging incoming WebSocket connections to the
//! `WsBroadcaster` event distribution system.

use std::sync::Arc;

use reinhardt::{ConsumerContext, Message, WebSocketConsumer, WebSocketResult};
use uuid::Uuid;

use crate::apps::auth::services::session::validate_raw_token;
use crate::apps::realtime::broadcaster::WsBroadcaster;
use crate::shared::ws_messages::{
	AuthResultPayload, LogStreamAckPayload, WsClientMessage, WsMessage,
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
	/// Authentication succeeded — register and send success response.
	AuthSuccess {
		user_id: String,
		response: WsMessage,
	},
	/// Authentication failed — send error response.
	AuthFailure { response: WsMessage },
	/// Subscribe to deployments (requires auth).
	Subscribe { deployment_ids: Vec<String> },
	/// Unsubscribe from deployments.
	Unsubscribe { deployment_ids: Vec<String> },
	/// Unauthenticated subscribe attempt — send error response.
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
			WsClientMessage::Authenticate { token } => match validate_raw_token(&token) {
				Some((uid, _username)) => ParsedAction::AuthSuccess {
					user_id: uid,
					response: WsMessage::AuthResult(AuthResultPayload {
						success: true,
						message: None,
					}),
				},
				None => ParsedAction::AuthFailure {
					response: WsMessage::AuthResult(AuthResultPayload {
						success: false,
						message: Some("Invalid or expired token".to_string()),
					}),
				},
			},
			WsClientMessage::Subscribe { deployment_ids } => {
				if user_id.is_none() {
					return ParsedAction::Rejected {
						response: WsMessage::AuthResult(AuthResultPayload {
							success: false,
							message: Some("Authentication required".to_string()),
						}),
					};
				}
				ParsedAction::Subscribe { deployment_ids }
			}
			WsClientMessage::Unsubscribe { deployment_ids } => {
				ParsedAction::Unsubscribe { deployment_ids }
			}
			// Log streaming subscriptions — acknowledged here, forwarded to gRPC
			// bridge in the async handler phase via the LogStreamAcknowledged action.
			WsClientMessage::SubscribeBuildLogs { .. }
			| WsClientMessage::SubscribeAppLogs { .. }
			| WsClientMessage::UnsubscribeLogs => {
				if user_id.is_none() {
					return ParsedAction::Rejected {
						response: WsMessage::AuthResult(AuthResultPayload {
							success: false,
							message: Some("Authentication required".to_string()),
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

		let action = Self::parse_client_message(user_id.as_deref(), client_msg);

		match action {
			ParsedAction::AuthSuccess {
				user_id: uid,
				response,
			} => {
				let _ = context.connection.send_json(&response).await;
				context
					.metadata
					.insert(META_USER_ID.to_string(), uid.clone());

				// Register the connection directly with the broadcaster.
				// The Room will hold an Arc<WebSocketConnection> and broadcast
				// to it without an intermediate forwarding task.
				self.broadcaster
					.register_connection(&connection_id, &uid, Arc::clone(&context.connection))
					.await;
			}
			ParsedAction::AuthFailure { response }
			| ParsedAction::Rejected { response }
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
	use serial_test::serial;

	#[rstest]
	#[serial(jwt)]
	fn test_parse_authenticate_with_invalid_token_returns_failure() {
		// Arrange
		// SAFETY: Tests using this helper are serialized with #[serial(jwt)]
		// to prevent concurrent environment variable mutation.
		unsafe {
			std::env::set_var(
				"REINHARDT_CLOUD_JWT_SECRET",
				"test-secret-key-for-unit-tests-minimum-length-32",
			);
		}

		let msg = WsClientMessage::Authenticate {
			token: "totally-invalid-jwt-token".to_string(),
		};

		// Act
		let action = NotificationConsumer::parse_client_message(None, msg);

		// Assert
		match action {
			ParsedAction::AuthFailure { response } => match &response {
				WsMessage::AuthResult(payload) => {
					assert!(!payload.success);
					assert!(payload.message.is_some());
				}
				_ => panic!("expected AuthResult"),
			},
			_ => panic!("expected AuthFailure action"),
		}
	}

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
				WsMessage::AuthResult(payload) => {
					assert!(!payload.success);
					assert_eq!(payload.message.as_deref(), Some("Authentication required"));
				}
				_ => panic!("expected AuthResult rejection"),
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
	#[serial(jwt)]
	fn test_parse_authenticate_empty_token() {
		// Arrange
		// SAFETY: Tests using this helper are serialized with #[serial(jwt)]
		// to prevent concurrent environment variable mutation.
		unsafe {
			std::env::set_var(
				"REINHARDT_CLOUD_JWT_SECRET",
				"test-secret-key-for-unit-tests-minimum-length-32",
			);
		}

		let msg = WsClientMessage::Authenticate {
			token: "".to_string(),
		};

		// Act
		let action = NotificationConsumer::parse_client_message(None, msg);

		// Assert
		match action {
			ParsedAction::AuthFailure { response } => match &response {
				WsMessage::AuthResult(payload) => {
					assert!(!payload.success);
					assert!(payload.message.is_some());
				}
				_ => panic!("expected AuthResult"),
			},
			_ => panic!("expected AuthFailure action"),
		}
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
