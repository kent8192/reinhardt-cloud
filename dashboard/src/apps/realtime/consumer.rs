//! WebSocket consumer for real-time notification delivery.
//!
//! `NotificationConsumer` implements the `WebSocketConsumer` trait from
//! reinhardt-websockets, bridging incoming WebSocket connections to the
//! `WsBroadcaster` event distribution system and the gRPC build log stream.

use std::sync::Arc;

use reinhardt::{ConsumerContext, Message, WebSocketConsumer, WebSocketResult};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use uuid::Uuid;

use reinhardt_cloud_proto::build as pb;

use crate::apps::auth::services::session::validate_raw_token;
use crate::apps::realtime::broadcaster::WsBroadcaster;
use crate::shared::ws_messages::{
	AuthResultPayload, BuildLogPayload, LogStreamAckPayload, WsClientMessage, WsMessage,
};

/// Metadata key for the connection UUID assigned during `on_connect`.
const META_CONNECTION_ID: &str = "connection_id";

/// Metadata key for the authenticated user ID (set after successful auth).
const META_USER_ID: &str = "user_id";

/// Default gRPC endpoint used when `GRPC_ENDPOINT` is not set.
const DEFAULT_GRPC_ENDPOINT: &str = "http://127.0.0.1:50051";

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
	/// Subscribe to build log events via the gRPC bridge.
	SubscribeBuildLogs { build_id: String },
	/// Subscribe to application log events (placeholder — no gRPC backend yet).
	SubscribeAppLogs { app_name: String },
	/// Cancel any active log stream.
	UnsubscribeLogs,
}

/// Resolve the gRPC endpoint from the environment or fall back to the default.
fn grpc_endpoint() -> String {
	std::env::var("GRPC_ENDPOINT").unwrap_or_else(|_| DEFAULT_GRPC_ENDPOINT.to_string())
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
	/// Active log streaming task handle. Protected by a `Mutex` so that only
	/// one log stream is active per consumer at a time. Subscribing to a new
	/// stream automatically cancels the previous one. Wrapped in `Arc` so
	/// that the spawned cleanup task can clear the handle when the stream
	/// finishes normally.
	log_stream_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl NotificationConsumer {
	/// Create a new consumer backed by the given broadcaster.
	pub fn new(broadcaster: Arc<WsBroadcaster>) -> Self {
		Self {
			broadcaster,
			log_stream_handle: Arc::new(Mutex::new(None)),
		}
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
			WsClientMessage::SubscribeBuildLogs { build_id } => {
				if user_id.is_none() {
					return ParsedAction::Rejected {
						response: WsMessage::AuthResult(AuthResultPayload {
							success: false,
							message: Some("Authentication required".to_string()),
						}),
					};
				}
				ParsedAction::SubscribeBuildLogs { build_id }
			}
			WsClientMessage::SubscribeAppLogs { app_name } => {
				if user_id.is_none() {
					return ParsedAction::Rejected {
						response: WsMessage::AuthResult(AuthResultPayload {
							success: false,
							message: Some("Authentication required".to_string()),
						}),
					};
				}
				ParsedAction::SubscribeAppLogs { app_name }
			}
			WsClientMessage::UnsubscribeLogs => {
				if user_id.is_none() {
					return ParsedAction::Rejected {
						response: WsMessage::AuthResult(AuthResultPayload {
							success: false,
							message: Some("Authentication required".to_string()),
						}),
					};
				}
				ParsedAction::UnsubscribeLogs
			}
		}
	}

	/// Cancel the active log stream, if any.
	async fn cancel_log_stream(&self) {
		let mut handle = self.log_stream_handle.lock().await;
		if let Some(h) = handle.take() {
			h.abort();
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
			ParsedAction::AuthFailure { response } | ParsedAction::Rejected { response } => {
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
			ParsedAction::SubscribeBuildLogs { build_id } => {
				// Cancel any previous log stream before starting a new one.
				self.cancel_log_stream().await;

				// Send acknowledgement immediately.
				let ack = WsMessage::LogStreamAck(LogStreamAckPayload {
					acknowledged: true,
					message: format!("Subscribed to build logs for {build_id}"),
				});
				let _ = context.connection.send_json(&ack).await;

				// Spawn a background task that connects to the gRPC
				// BuildService and forwards log entries as WebSocket messages.
				let conn = Arc::clone(&context.connection);
				let bid = build_id.clone();
				let endpoint = grpc_endpoint();

				let handle_ref = Arc::clone(&self.log_stream_handle);

				let handle = tokio::spawn(async move {
					let mut client =
						match pb::build_service_client::BuildServiceClient::connect(endpoint).await
						{
							Ok(c) => c,
							Err(e) => {
								tracing::warn!(
									build_id = %bid,
									error = %e,
									"Failed to connect to gRPC BuildService for log streaming",
								);
								// Notify client that the stream could not be established.
								let err_msg = WsMessage::LogStreamAck(LogStreamAckPayload {
									acknowledged: false,
									message: format!(
										"Failed to connect to build log service for {bid}: {e}"
									),
								});
								let _ = conn.send_json(&err_msg).await;
								// Clear handle on exit.
								*handle_ref.lock().await = None;
								return;
							}
						};

					let request = pb::StreamBuildLogsRequest {
						build_id: bid.clone(),
						follow: true,
					};

					let response = match client.stream_build_logs(request).await {
						Ok(r) => r,
						Err(e) => {
							tracing::warn!(
								build_id = %bid,
								error = %e,
								"gRPC StreamBuildLogs call failed",
							);
							// Notify client that the stream call failed.
							let err_msg = WsMessage::LogStreamAck(LogStreamAckPayload {
								acknowledged: false,
								message: format!("Build log stream failed for {bid}: {e}"),
							});
							let _ = conn.send_json(&err_msg).await;
							// Clear handle on exit.
							*handle_ref.lock().await = None;
							return;
						}
					};

					let mut stream = response.into_inner();

					loop {
						match stream.message().await {
							Ok(Some(log)) => {
								let ts = log
									.timestamp
									.map(|t| {
										// Validate nanos is within the valid range
										// (0..=999_999_999). Out-of-range values
										// (including negatives from prost_types)
										// are clamped to 0.
										let nanos = if (0..=999_999_999).contains(&t.nanos) {
											t.nanos as u32
										} else {
											0
										};
										chrono::DateTime::from_timestamp(t.seconds, nanos)
											.map(|dt| dt.to_rfc3339())
											.unwrap_or_default()
									})
									.unwrap_or_default();

								let ws_msg = WsMessage::BuildLog(BuildLogPayload {
									build_id: bid.clone(),
									event_type: "log".to_string(),
									message: log.message,
									timestamp: ts,
								});

								if conn.send_json(&ws_msg).await.is_err() {
									// Connection closed — stop streaming.
									break;
								}
							}
							Ok(None) => {
								// Stream ended normally.
								break;
							}
							Err(e) => {
								tracing::warn!(
									build_id = %bid,
									error = %e,
									"Error receiving build log from gRPC stream",
								);
								break;
							}
						}
					}

					// Clear the stale handle when the stream exits normally.
					*handle_ref.lock().await = None;
				});

				// Store the streaming task handle directly so that
				// cancel_log_stream() aborts the actual gRPC stream task.
				*self.log_stream_handle.lock().await = Some(handle);
			}
			ParsedAction::SubscribeAppLogs { app_name } => {
				// Cancel any previous log stream before acknowledging.
				self.cancel_log_stream().await;

				// App log streaming is not yet available in the gRPC proto.
				// Send an acknowledgement so the client knows the request was
				// received and can display a placeholder.
				let ack = WsMessage::LogStreamAck(LogStreamAckPayload {
					acknowledged: true,
					message: format!(
						"App log streaming for '{app_name}' acknowledged (not yet available)"
					),
				});
				let _ = context.connection.send_json(&ack).await;
			}
			ParsedAction::UnsubscribeLogs => {
				self.cancel_log_stream().await;

				let ack = WsMessage::LogStreamAck(LogStreamAckPayload {
					acknowledged: true,
					message: "Log stream unsubscribed".to_string(),
				});
				let _ = context.connection.send_json(&ack).await;
			}
		}

		Ok(())
	}

	async fn on_disconnect(&self, context: &mut ConsumerContext) -> WebSocketResult<()> {
		// Cancel any active log stream for this connection.
		self.cancel_log_stream().await;

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

	// --- Tests for new log streaming ParsedAction variants ---

	#[rstest]
	fn test_parse_subscribe_build_logs_with_auth() {
		// Arrange
		let msg = WsClientMessage::SubscribeBuildLogs {
			build_id: "build-42".to_string(),
		};

		// Act
		let action = NotificationConsumer::parse_client_message(Some("user-1"), msg);

		// Assert
		match action {
			ParsedAction::SubscribeBuildLogs { build_id } => {
				assert_eq!(build_id, "build-42");
			}
			_ => panic!("expected SubscribeBuildLogs action"),
		}
	}

	#[rstest]
	fn test_parse_subscribe_build_logs_without_auth_rejected() {
		// Arrange
		let msg = WsClientMessage::SubscribeBuildLogs {
			build_id: "build-42".to_string(),
		};

		// Act
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
	fn test_parse_subscribe_app_logs_with_auth() {
		// Arrange
		let msg = WsClientMessage::SubscribeAppLogs {
			app_name: "my-service".to_string(),
		};

		// Act
		let action = NotificationConsumer::parse_client_message(Some("user-1"), msg);

		// Assert
		match action {
			ParsedAction::SubscribeAppLogs { app_name } => {
				assert_eq!(app_name, "my-service");
			}
			_ => panic!("expected SubscribeAppLogs action"),
		}
	}

	#[rstest]
	fn test_parse_subscribe_app_logs_without_auth_rejected() {
		// Arrange
		let msg = WsClientMessage::SubscribeAppLogs {
			app_name: "my-service".to_string(),
		};

		// Act
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
	fn test_parse_unsubscribe_logs_with_auth() {
		// Arrange
		let msg = WsClientMessage::UnsubscribeLogs;

		// Act
		let action = NotificationConsumer::parse_client_message(Some("user-1"), msg);

		// Assert
		matches!(action, ParsedAction::UnsubscribeLogs);
	}

	#[rstest]
	fn test_parse_unsubscribe_logs_without_auth_rejected() {
		// Arrange
		let msg = WsClientMessage::UnsubscribeLogs;

		// Act
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
}
