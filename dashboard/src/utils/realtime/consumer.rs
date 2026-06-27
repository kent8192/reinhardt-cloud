//! WebSocket consumer for real-time notification delivery.
//!
//! `NotificationConsumer` implements the `WebSocketConsumer` trait from
//! reinhardt-websockets, bridging incoming WebSocket connections to the
//! `WsBroadcaster` event distribution system and the gRPC build log stream.

use std::sync::Arc;

use reinhardt::{
	ConsumerContext, Message, Model, WebSocketConsumer, WebSocketError, WebSocketResult,
};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use uuid::Uuid;

use reinhardt_cloud_proto::build as pb;
use reinhardt_cloud_proto::log as log_pb;
use reinhardt_cloud_types::crd::tenant::TenantRef;

use crate::apps::auth::services::session::validate_session;
use crate::apps::deployments::models::Deployment;
use crate::apps::organizations::models::Organization;
use crate::apps::organizations::permissions::action::Action;
use crate::apps::organizations::permissions::guard::require_permission;
use crate::config::settings::get_settings;
use crate::shared::ws_messages::{
	AppLogPayload, BuildLogPayload, LogStreamAckPayload, NotificationLevel,
	SystemNotificationPayload, WsClientMessage, WsMessage,
};
use crate::utils::realtime::broadcaster::WsBroadcaster;

/// Metadata key for the connection UUID assigned during `on_connect`.
const META_CONNECTION_ID: &str = "connection_id";

/// Metadata key for the authenticated user ID (set after successful auth).
const META_USER_ID: &str = "user_id";

/// Default gRPC endpoint used when `GRPC_ENDPOINT` is not set.
const DEFAULT_GRPC_ENDPOINT: &str = "http://127.0.0.1:50051";

/// Generic client-facing failure for unavailable build-log streams.
const BUILD_LOG_STREAM_UNAVAILABLE: &str = "Build log stream is currently unavailable";

/// Generic client-facing failure for unavailable application-log streams.
const APP_LOG_STREAM_UNAVAILABLE: &str = "Application log stream is currently unavailable";

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
	/// Subscribe to build log events via the gRPC bridge.
	SubscribeBuildLogs { build_id: String },
	/// Subscribe to application log events via the gRPC `LogService` bridge.
	SubscribeAppLogs { deployment_id: String },
	/// Acknowledged log stream subscription — send ack and start/stop stream.
	/// Currently unused; will be wired when build log streaming is connected to the match arm.
	#[allow(dead_code)]
	LogStreamAcknowledged { response: WsMessage },
	/// Cancel any active log stream.
	UnsubscribeLogs,
}

/// Resolve the gRPC endpoint from the environment or fall back to the default.
fn grpc_endpoint() -> String {
	std::env::var("GRPC_ENDPOINT").unwrap_or_else(|_| DEFAULT_GRPC_ENDPOINT.to_string())
}

/// Convert a proto `LogLevel` enum value to its lowercase string form.
fn proto_log_level_str(level: i32) -> &'static str {
	match log_pb::LogLevel::try_from(level) {
		Ok(log_pb::LogLevel::Debug) => "debug",
		Ok(log_pb::LogLevel::Warn) => "warn",
		Ok(log_pb::LogLevel::Error) => "error",
		_ => "info",
	}
}

/// Convert a proto `LogEntry` into the dashboard's `AppLogPayload`.
///
/// Out-of-range or negative `nanos` values are clamped to 0; invalid
/// timestamps produce an empty RFC3339 string rather than a panic.
fn proto_entry_to_app_log(entry: &log_pb::LogEntry) -> AppLogPayload {
	let timestamp = entry
		.timestamp
		.map(|t| {
			let nanos = if (0..=999_999_999).contains(&t.nanos) {
				t.nanos as u32
			} else {
				0
			};
			chrono::DateTime::<chrono::Utc>::from_timestamp(t.seconds, nanos)
				.map(|dt| dt.to_rfc3339())
				.unwrap_or_default()
		})
		.unwrap_or_default();

	AppLogPayload {
		source: entry.source.clone(),
		level: proto_log_level_str(entry.level).to_string(),
		message: entry.message.clone(),
		timestamp,
		metadata: entry
			.metadata_json
			.as_ref()
			.and_then(|s| serde_json::from_str(s).ok()),
	}
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
			WsClientMessage::Subscribe { deployment_ids } => {
				if user_id.is_none() {
					return ParsedAction::Rejected {
						response: WsMessage::SystemNotification(SystemNotificationPayload {
							id: uuid::Uuid::now_v7().to_string(),
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
			WsClientMessage::SubscribeBuildLogs { build_id: _ } => {
				if user_id.is_none() {
					return ParsedAction::Rejected {
						response: WsMessage::SystemNotification(SystemNotificationPayload {
							id: String::new(),
							level: NotificationLevel::Warning,
							title: "Unauthorized".to_string(),
							message: "Authentication required".to_string(),
							timestamp: chrono::Utc::now().to_rfc3339(),
						}),
					};
				}
				ParsedAction::Rejected {
					response: log_stream_rejected(
						"Build log streaming is unavailable until build ownership can be verified",
					),
				}
			}
			WsClientMessage::SubscribeAppLogs { deployment_id } => {
				if user_id.is_none() {
					return ParsedAction::Rejected {
						response: WsMessage::SystemNotification(SystemNotificationPayload {
							id: String::new(),
							level: NotificationLevel::Warning,
							title: "Unauthorized".to_string(),
							message: "Authentication required".to_string(),
							timestamp: chrono::Utc::now().to_rfc3339(),
						}),
					};
				}
				ParsedAction::SubscribeAppLogs { deployment_id }
			}
			WsClientMessage::UnsubscribeLogs => {
				if user_id.is_none() {
					return ParsedAction::Rejected {
						response: WsMessage::SystemNotification(SystemNotificationPayload {
							id: String::new(),
							level: NotificationLevel::Warning,
							title: "Unauthorized".to_string(),
							message: "Authentication required".to_string(),
							timestamp: chrono::Utc::now().to_rfc3339(),
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct AuthorizedAppLogSubscription {
	deployment_id: i64,
	project_name: String,
	namespace: String,
}

fn log_stream_rejected(message: impl Into<String>) -> WsMessage {
	WsMessage::LogStreamAck(LogStreamAckPayload {
		acknowledged: false,
		message: message.into(),
	})
}

fn build_log_stream_unavailable() -> WsMessage {
	log_stream_rejected(BUILD_LOG_STREAM_UNAVAILABLE)
}

fn app_log_stream_unavailable() -> WsMessage {
	log_stream_rejected(APP_LOG_STREAM_UNAVAILABLE)
}

async fn authorize_app_log_subscription(
	user_id: &str,
	deployment_id: &str,
) -> Result<AuthorizedAppLogSubscription, WsMessage> {
	let user_id = Uuid::parse_str(user_id)
		.map_err(|_| log_stream_rejected("Authentication required for app logs"))?;
	let deployment_id: i64 = deployment_id
		.parse()
		.map_err(|_| log_stream_rejected("Invalid deployment id for app logs"))?;
	let organization_id = require_permission(user_id, Action::LogsRead)
		.await
		.map_err(|_| log_stream_rejected("Not authorized to read deployment logs"))?;
	let organization = Organization::objects()
		.filter(Organization::field_id().eq(organization_id))
		.first()
		.await
		.map_err(|e| {
			tracing::error!(
				error = %e,
				"Failed to load organization for app-log subscription"
			);
			log_stream_rejected("Failed to load organization for logs")
		})?
		.ok_or_else(|| log_stream_rejected("Organization not found for log subscription"))?;
	let namespace = TenantRef {
		organization: organization.slug,
		team: None,
	}
	.namespace();
	let deployment = Deployment::objects()
		.filter(Deployment::field_id().eq(deployment_id))
		.filter(Deployment::field_organization_id().eq(organization_id))
		.first()
		.await
		.map_err(|e| {
			tracing::error!(
				error = %e,
				"Failed to load deployment for app-log subscription"
			);
			log_stream_rejected("Failed to load deployment for logs")
		})?
		.ok_or_else(|| log_stream_rejected("Deployment not found for log subscription"))?;
	Ok(AuthorizedAppLogSubscription {
		deployment_id,
		project_name: deployment.project_name,
		namespace,
	})
}

/// Normalize an origin string for exact origin allow-list comparison.
fn normalize_origin(origin: &str) -> String {
	origin.trim().trim_end_matches('/').to_lowercase()
}

/// Return the dashboard WebSocket origin allow-list derived from CORS settings.
fn websocket_allowed_origins() -> Vec<String> {
	let settings = get_settings();
	let mut origins = Vec::new();
	for origin in &settings.cors.allow_origins {
		if origin != "*" {
			origins.push(normalize_origin(origin));
		}
	}

	if settings.core.debug {
		let port = std::env::var("PORT").unwrap_or_else(|_| "8000".to_string());
		origins.push(normalize_origin(&format!("http://localhost:{port}")));
		origins.push(normalize_origin(&format!("http://127.0.0.1:{port}")));
	}

	origins.sort();
	origins.dedup();
	origins
}

/// Validate the WebSocket handshake `Origin` header before using cookies.
fn validate_websocket_origin(context: &ConsumerContext) -> WebSocketResult<()> {
	let origin = context
		.get_header("origin")
		.map(|value| normalize_origin(value))
		.filter(|value| !value.is_empty())
		.ok_or_else(|| WebSocketError::Connection("Missing WebSocket Origin header".to_string()))?;

	let allowed_origins = websocket_allowed_origins();
	if allowed_origins.iter().any(|allowed| allowed == &origin) {
		Ok(())
	} else {
		Err(WebSocketError::Connection(
			"WebSocket Origin is not allowed".to_string(),
		))
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
		let connection_id = Uuid::now_v7().to_string();
		context
			.metadata
			.insert(META_CONNECTION_ID.to_string(), connection_id.clone());

		validate_websocket_origin(context)?;

		// Authenticate from session cookie in handshake headers after origin validation.
		if let Some(cookie_header) = context.cookie_header()
			&& let Some(session_id) = extract_cookie_value(cookie_header, "sessionid")
			&& let Some((user_id, _username)) = validate_session(&session_id).await
		{
			context
				.metadata
				.insert(META_USER_ID.to_string(), user_id.clone());
			self.broadcaster
				.register_connection(&connection_id, &user_id, Arc::clone(&context.connection))
				.await;
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
			ParsedAction::SubscribeBuildLogs { build_id } => {
				// Cancel any previous log stream before starting a new one.
				self.cancel_log_stream().await;

				// Spawn a background task that connects to the gRPC
				// BuildService and forwards log entries as WebSocket messages.
				// The positive acknowledgement is sent only after the gRPC
				// connection is established, so the client is not misled when
				// the connection subsequently fails.
				let conn = Arc::clone(&context.connection);
				let bid = build_id.clone();
				let endpoint = grpc_endpoint();

				let handle_ref = Arc::clone(&self.log_stream_handle);

				// Acquire the lock before spawning so that the task cannot
				// clear the handle before we store it (fixes the race where a
				// very fast task completion sets handle_ref to None before
				// the outer code sets it to Some(handle)).
				let mut handle_guard = self.log_stream_handle.lock().await;

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
								let err_msg = build_log_stream_unavailable();
								let _ = conn.send_json(&err_msg).await;
								// Clear handle on exit.
								*handle_ref.lock().await = None;
								return;
							}
						};

					// Send positive acknowledgement only after a successful
					// gRPC connection — avoids a contradictory ack/nack pair.
					let ack = WsMessage::LogStreamAck(LogStreamAckPayload {
						acknowledged: true,
						message: format!("Subscribed to build logs for {bid}"),
					});
					let _ = conn.send_json(&ack).await;

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
							let err_msg = build_log_stream_unavailable();
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
										chrono::DateTime::<chrono::Utc>::from_timestamp(
											t.seconds, nanos,
										)
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

				// Store the handle while still holding the lock so that a
				// fast-finishing task cannot set the handle to None before
				// we write Some(handle).
				*handle_guard = Some(handle);
				drop(handle_guard);
			}
			ParsedAction::SubscribeAppLogs { deployment_id } => {
				let Some(uid) = user_id.as_deref() else {
					let _ = context
						.connection
						.send_json(&log_stream_rejected("Authentication required for app logs"))
						.await;
					return Ok(());
				};
				let subscription = match authorize_app_log_subscription(uid, &deployment_id).await {
					Ok(subscription) => subscription,
					Err(response) => {
						let _ = context.connection.send_json(&response).await;
						return Ok(());
					}
				};
				// Cancel any previous log stream before starting a new one.
				self.cancel_log_stream().await;

				// Spawn a background task that connects to the gRPC
				// LogService and forwards tailed log entries as WebSocket
				// messages.
				// The positive acknowledgement is sent only after the gRPC
				// connection is established, so the client is not misled when
				// the connection subsequently fails.
				let conn = Arc::clone(&context.connection);
				let project = subscription.project_name.clone();
				let namespace = subscription.namespace.clone();
				let endpoint = grpc_endpoint();
				let handle_ref = Arc::clone(&self.log_stream_handle);

				// Acquire the lock before spawning to prevent the race where
				// the task clears the handle before we store it.
				let mut handle_guard = self.log_stream_handle.lock().await;

				let handle = tokio::spawn(async move {
					let mut client =
						match log_pb::log_service_client::LogServiceClient::connect(endpoint).await
						{
							Ok(c) => c,
							Err(e) => {
								tracing::warn!(
									project_name = %project,
									error = %e,
									"Failed to connect to gRPC LogService for app log streaming",
								);
								let err_msg = app_log_stream_unavailable();
								let _ = conn.send_json(&err_msg).await;
								*handle_ref.lock().await = None;
								return;
							}
						};

					let request = log_pb::TailLogsRequest {
						filter: Some(log_pb::LogFilter {
							source: Some(project.clone()),
							namespace: Some(namespace.clone()),
							..Default::default()
						}),
					};

					// Send positive acknowledgement only after tail_logs succeeds,
					// so the client is never misled by a success ack followed by a
					// failure ack when the RPC itself fails.
					let mut stream = match client.tail_logs(request).await {
						Ok(r) => r.into_inner(),
						Err(e) => {
							tracing::warn!(
								project_name = %project,
								error = %e,
								"gRPC TailLogs call failed",
							);
							let err_msg = app_log_stream_unavailable();
							let _ = conn.send_json(&err_msg).await;
							*handle_ref.lock().await = None;
							return;
						}
					};

					let ack = WsMessage::LogStreamAck(LogStreamAckPayload {
						acknowledged: true,
						message: format!("Subscribed to app logs for {project}"),
					});
					let _ = conn.send_json(&ack).await;

					loop {
						match stream.message().await {
							Ok(Some(entry)) => {
								let ws_msg = WsMessage::AppLog(proto_entry_to_app_log(&entry));
								if conn.send_json(&ws_msg).await.is_err() {
									// Connection closed — stop streaming.
									break;
								}
							}
							Ok(None) => break,
							Err(e) => {
								tracing::warn!(
									project_name = %project,
									error = %e,
									"Error receiving app log from gRPC stream",
								);
								break;
							}
						}
					}

					// Clear the stale handle when the stream exits normally.
					*handle_ref.lock().await = None;
				});

				// Store the handle while still holding the lock so that a
				// fast-finishing task cannot set the handle to None before
				// we write Some(handle).
				*handle_guard = Some(handle);
				drop(handle_guard);
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
	use chrono::Utc;
	use reinhardt::prelude::DatabaseConnection;
	use reinhardt::test::fixtures::{
		ContainerAsync, GenericImage, postgres_with_migrations_from_dir,
	};
	use rstest::fixture;
	use rstest::rstest;
	use serial_test::serial;

	use crate::apps::auth::models::User;
	use crate::apps::clusters::models::Cluster;
	use crate::apps::organizations::models::{Organization, OrganizationMembership};
	use crate::apps::organizations::roles::MembershipRole;

	#[fixture]
	async fn db() -> (ContainerAsync<GenericImage>, Arc<DatabaseConnection>) {
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations")
	}

	async fn create_user(conn: &Arc<DatabaseConnection>, username: &str) -> User {
		User::objects()
			.create_with_conn(
				conn,
				&User::build()
					.username(username.to_string())
					.email(format!("{username}@example.com"))
					.first_name(String::new())
					.last_name(String::new())
					.password_hash(None)
					.is_active(true)
					.is_staff(false)
					.is_superuser(false)
					.finish(),
			)
			.await
			.expect("create user")
	}

	async fn create_org(
		conn: &Arc<DatabaseConnection>,
		creator: &User,
		slug: &str,
	) -> Organization {
		let now = Utc::now();
		Organization::objects()
			.create_with_conn(
				conn,
				&Organization {
					id: None,
					slug: slug.to_string(),
					name: slug.to_string(),
					created_by: creator.id,
					created_at: now,
					updated_at: now,
				},
			)
			.await
			.expect("create org")
	}

	async fn add_membership(conn: &Arc<DatabaseConnection>, user: &User, org: &Organization) {
		OrganizationMembership::objects()
			.create_with_conn(
				conn,
				&OrganizationMembership::build()
					.organization(org.id.expect("created org has id"))
					.user(user.id)
					.role(MembershipRole::Viewer.as_db_str().to_string())
					.finish(),
			)
			.await
			.expect("create membership");
	}

	async fn create_deployment(
		conn: &Arc<DatabaseConnection>,
		org: &Organization,
		project_name: &str,
	) -> Deployment {
		let cluster = Cluster::objects()
			.create_with_conn(
				conn,
				&Cluster::build()
					.organization(org.id.expect("created org has id"))
					.name(format!("{project_name}-cluster"))
					.api_url("https://k8s.example.com".to_string())
					.is_active(true)
					.token_hash(None)
					.token_last_rotated_at(None)
					.finish(),
			)
			.await
			.expect("create cluster");
		Deployment::objects()
			.create_with_conn(
				conn,
				&Deployment::build()
					.organization(org.id.expect("created org has id"))
					.project_name(project_name.to_string())
					.cluster(cluster.id.expect("created cluster has id"))
					.status("running".to_string())
					.image("ghcr.io/example/app:latest".to_string())
					.project_yaml(None)
					.finish(),
			)
			.await
			.expect("create deployment")
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
	fn test_normalize_origin_trims_trailing_slash_and_case() {
		// Arrange
		let origin = " HTTPS://Example.COM/ ";

		// Act
		let normalized = normalize_origin(origin);

		// Assert
		assert_eq!(normalized, "https://example.com");
	}

	#[rstest]
	fn test_validate_websocket_origin_rejects_missing_origin() {
		// Arrange
		let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
		let conn = Arc::new(reinhardt::WebSocketConnection::new(
			"conn-1".to_string(),
			tx,
		));
		let context = ConsumerContext::new(conn);

		// Act
		let result = validate_websocket_origin(&context);

		// Assert
		assert!(matches!(result, Err(WebSocketError::Connection(_))));
	}

	#[rstest]
	fn test_validate_websocket_origin_allows_configured_origin() {
		// Arrange
		let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
		let conn = Arc::new(reinhardt::WebSocketConnection::new(
			"conn-1".to_string(),
			tx,
		));
		let context = ConsumerContext::new(conn)
			.with_header("origin".to_string(), "http://localhost:8000".to_string());

		// Act
		let result = validate_websocket_origin(&context);

		// Assert
		assert!(result.is_ok());
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
	fn test_build_log_stream_unavailable_uses_generic_message() {
		// Arrange and Act
		let response = build_log_stream_unavailable();

		// Assert
		match response {
			WsMessage::LogStreamAck(payload) => {
				assert_eq!(payload.acknowledged, false);
				assert_eq!(payload.message, BUILD_LOG_STREAM_UNAVAILABLE);
			}
			_ => panic!("expected LogStreamAck response"),
		}
	}

	#[rstest]
	fn test_app_log_stream_unavailable_uses_generic_message() {
		// Arrange and Act
		let response = app_log_stream_unavailable();

		// Assert
		match response {
			WsMessage::LogStreamAck(payload) => {
				assert_eq!(payload.acknowledged, false);
				assert_eq!(payload.message, APP_LOG_STREAM_UNAVAILABLE);
			}
			_ => panic!("expected LogStreamAck response"),
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
	fn test_parse_subscribe_build_logs_with_auth_rejected_until_ownership_verified() {
		// Arrange
		let msg = WsClientMessage::SubscribeBuildLogs {
			build_id: "build-42".to_string(),
		};

		// Act
		let action = NotificationConsumer::parse_client_message(Some("user-1"), msg);

		// Assert
		match action {
			ParsedAction::Rejected { response } => match &response {
				WsMessage::LogStreamAck(payload) => {
					assert_eq!(payload.acknowledged, false);
					assert_eq!(
						payload.message,
						"Build log streaming is unavailable until build ownership can be verified"
					);
				}
				_ => panic!("expected LogStreamAck rejection"),
			},
			_ => panic!("expected Rejected action"),
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
				WsMessage::SystemNotification(payload) => {
					assert_eq!(payload.level, NotificationLevel::Warning);
					assert_eq!(payload.message, "Authentication required");
				}
				_ => panic!("expected SystemNotification rejection"),
			},
			_ => panic!("expected Rejected action"),
		}
	}

	#[rstest]
	fn test_parse_subscribe_app_logs_with_auth() {
		// Arrange
		let msg = WsClientMessage::SubscribeAppLogs {
			deployment_id: "42".to_string(),
		};

		// Act
		let action = NotificationConsumer::parse_client_message(Some("user-1"), msg);

		// Assert
		match action {
			ParsedAction::SubscribeAppLogs { deployment_id } => {
				assert_eq!(deployment_id, "42");
			}
			_ => panic!("expected SubscribeAppLogs action"),
		}
	}

	#[rstest]
	fn test_parse_subscribe_app_logs_without_auth_rejected() {
		// Arrange
		let msg = WsClientMessage::SubscribeAppLogs {
			deployment_id: "42".to_string(),
		};

		// Act
		let action = NotificationConsumer::parse_client_message(None, msg);

		// Assert
		match action {
			ParsedAction::Rejected { response } => match &response {
				WsMessage::SystemNotification(payload) => {
					assert_eq!(payload.level, NotificationLevel::Warning);
					assert_eq!(payload.message, "Authentication required");
				}
				_ => panic!("expected SystemNotification rejection"),
			},
			_ => panic!("expected Rejected action"),
		}
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn authorize_app_log_subscription_allows_deployment_in_current_org(
		#[future] db: (ContainerAsync<GenericImage>, Arc<DatabaseConnection>),
	) {
		// Arrange
		let (_container, conn) = db.await;
		let user = create_user(&conn, "logs-allowed").await;
		let org = create_org(&conn, &user, "logs-allowed").await;
		add_membership(&conn, &user, &org).await;
		let deployment = create_deployment(&conn, &org, "allowed-project").await;

		// Act
		let subscription = authorize_app_log_subscription(
			&user.id.to_string(),
			&deployment.id.unwrap().to_string(),
		)
		.await
		.expect("authorized subscription");

		// Assert
		assert_eq!(subscription.deployment_id, deployment.id.unwrap());
		assert_eq!(subscription.project_name, "allowed-project");
		assert_eq!(subscription.namespace, "tenant-logs-allowed");
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn authorize_app_log_subscription_rejects_user_without_membership(
		#[future] db: (ContainerAsync<GenericImage>, Arc<DatabaseConnection>),
	) {
		// Arrange
		let (_container, conn) = db.await;
		let owner = create_user(&conn, "logs-owner").await;
		let user = create_user(&conn, "logs-stranded").await;
		let org = create_org(&conn, &owner, "logs-owner").await;
		let deployment = create_deployment(&conn, &org, "owned-project").await;

		// Act
		let result = authorize_app_log_subscription(
			&user.id.to_string(),
			&deployment.id.unwrap().to_string(),
		)
		.await;

		// Assert
		match result {
			Err(WsMessage::LogStreamAck(payload)) => {
				assert_eq!(payload.acknowledged, false);
				assert_eq!(payload.message, "Not authorized to read deployment logs");
			}
			_ => panic!("expected rejected LogStreamAck"),
		}
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn authorize_app_log_subscription_rejects_cross_org_deployment_guess(
		#[future] db: (ContainerAsync<GenericImage>, Arc<DatabaseConnection>),
	) {
		// Arrange
		let (_container, conn) = db.await;
		let user = create_user(&conn, "logs-user").await;
		let other_user = create_user(&conn, "logs-other").await;
		let own_org = create_org(&conn, &user, "logs-user").await;
		let other_org = create_org(&conn, &other_user, "logs-other").await;
		add_membership(&conn, &user, &own_org).await;
		add_membership(&conn, &other_user, &other_org).await;
		let other_deployment = create_deployment(&conn, &other_org, "other-project").await;

		// Act
		let result = authorize_app_log_subscription(
			&user.id.to_string(),
			&other_deployment.id.unwrap().to_string(),
		)
		.await;

		// Assert
		match result {
			Err(WsMessage::LogStreamAck(payload)) => {
				assert_eq!(payload.acknowledged, false);
				assert_eq!(payload.message, "Deployment not found for log subscription");
			}
			_ => panic!("expected rejected LogStreamAck"),
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
				WsMessage::SystemNotification(payload) => {
					assert_eq!(payload.level, NotificationLevel::Warning);
					assert_eq!(payload.message, "Authentication required");
				}
				_ => panic!("expected SystemNotification rejection"),
			},
			_ => panic!("expected Rejected action"),
		}
	}
}
