//! WebSocket connection manager for the WASM client.
//!
//! Establishes a single global connection to `/ws/notifications`
//! and dispatches incoming messages to toast notifications and
//! status badge updates. Authentication is handled via session
//! cookies sent automatically with the WebSocket handshake.

#[cfg(wasm)]
use std::cell::{Cell, RefCell};
#[cfg(wasm)]
use std::collections::HashSet;

#[cfg(wasm)]
use wasm_bindgen::JsCast;
#[cfg(wasm)]
use wasm_bindgen::prelude::*;
#[cfg(wasm)]
use web_sys::{MessageEvent, WebSocket};

#[cfg(wasm)]
use crate::shared::ws_messages::{
	DeploymentState, DeploymentStatusPayload, MAX_SUBSCRIPTIONS_PER_USER, NotificationLevel,
	WsClientMessage, WsMessage,
};

#[cfg(wasm)]
use super::components::status_badge;
#[cfg(wasm)]
use super::components::toast::show_toast;
#[cfg(wasm)]
use super::style::STYLES;
#[cfg(wasm)]
use crate::apps::deployments::client::components::{cluster_health, log_viewer};

#[cfg(wasm)]
thread_local! {
	static SUBSCRIBED_IDS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
	static APP_LOG_DEPLOYMENT_ID: RefCell<Option<String>> = const { RefCell::new(None) };
	static RECONNECT_ATTEMPTS: RefCell<u32> = const { RefCell::new(0) };
	static CURRENT_WS: RefCell<Option<NotificationConnection>> = const { RefCell::new(None) };
	static RECONNECT_TIMEOUT: RefCell<Option<gloo_timers::callback::Timeout>> = const { RefCell::new(None) };
	static NOTIFICATIONS_ENABLED: Cell<bool> = const { Cell::new(false) };
}

/// Owns the socket and its callbacks for exactly one authenticated connection.
#[cfg(wasm)]
struct NotificationConnection {
	socket: WebSocket,
	_on_open: Closure<dyn FnMut(web_sys::Event)>,
	_on_message: Closure<dyn FnMut(MessageEvent)>,
	_on_close: Closure<dyn FnMut(web_sys::Event)>,
}

#[cfg(wasm)]
impl Drop for NotificationConnection {
	fn drop(&mut self) {
		self.socket.set_onopen(None);
		self.socket.set_onmessage(None);
		self.socket.set_onclose(None);
		let _ = self.socket.close();
	}
}

#[cfg(wasm)]
const MAX_RECONNECT_ATTEMPTS: u32 = 10;

/// Return whether a SPA path should keep the authenticated notifications
/// WebSocket connected.
pub fn should_connect_notifications_for_path(path: &str) -> bool {
	matches!(path, "/" | "/clusters" | "/deployments")
}

/// Open a WebSocket to `/ws/notifications` and wire up event handlers.
///
/// Session cookies are sent automatically with the WebSocket handshake,
/// so no explicit authentication message is needed. On open, the
/// connection re-subscribes to any deployment IDs previously registered
/// via [`track_subscriptions`] and any app-log deployment registered via
/// [`subscribe_app_logs`].
///
/// Automatically reconnects on close up to [`MAX_RECONNECT_ATTEMPTS`]
/// times with a fixed 3-second delay.
#[cfg(wasm)]
pub fn connect_notifications() {
	if !NOTIFICATIONS_ENABLED.with(Cell::get) {
		return;
	}
	RECONNECT_TIMEOUT.with(|timer| timer.borrow_mut().take());
	CURRENT_WS.with(|current| current.borrow_mut().take());

	let window = web_sys::window().unwrap();
	let location = window.location();
	let host = location.host().unwrap();
	let protocol = if location.protocol().unwrap() == "https:" {
		"wss:"
	} else {
		"ws:"
	};
	let url = format!("{protocol}//{host}/ws/notifications");
	let Ok(ws) = WebSocket::new(&url) else {
		return;
	};

	// On open: reset reconnect counter, re-subscribe to tracked deployments
	let ws_for_open = ws.clone();
	let on_open = Closure::wrap(Box::new(move |_: web_sys::Event| {
		RECONNECT_ATTEMPTS.with(|c| *c.borrow_mut() = 0);

		SUBSCRIBED_IDS.with(|ids| {
			let ids = ids.borrow().iter().cloned().collect::<Vec<_>>();
			for chunk in ids.chunks(MAX_SUBSCRIPTIONS_PER_USER) {
				let sub = WsClientMessage::Subscribe {
					deployment_ids: chunk.to_vec(),
				};
				send_client_message(&ws_for_open, &sub);
			}
		});

		APP_LOG_DEPLOYMENT_ID.with(|deployment_id| {
			if let Some(deployment_id) = deployment_id.borrow().as_deref() {
				send_client_message(
					&ws_for_open,
					&WsClientMessage::SubscribeAppLogs {
						deployment_id: deployment_id.to_string(),
					},
				);
			}
		});
	}) as Box<dyn FnMut(_)>);
	ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));

	// On message: deserialize and dispatch
	let on_message = Closure::wrap(Box::new(move |event: MessageEvent| {
		let Some(data) = event.data().as_string() else {
			return;
		};
		let Ok(msg) = serde_json::from_str::<WsMessage>(&data) else {
			return;
		};
		handle_ws_message(msg);
	}) as Box<dyn FnMut(_)>);
	ws.set_onmessage(Some(on_message.as_ref().unchecked_ref()));

	// On close: auto-reconnect with attempt limit
	let on_close = Closure::wrap(Box::new(move |_: web_sys::Event| {
		let should_reconnect = RECONNECT_ATTEMPTS.with(|c| {
			let mut count = c.borrow_mut();
			*count += 1;
			*count <= MAX_RECONNECT_ATTEMPTS
		});
		if should_reconnect && NOTIFICATIONS_ENABLED.with(Cell::get) {
			RECONNECT_TIMEOUT.with(|timer| {
				*timer.borrow_mut() = Some(gloo_timers::callback::Timeout::new(
					3_000,
					connect_notifications,
				));
			});
		}
	}) as Box<dyn FnMut(_)>);
	ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
	CURRENT_WS.with(|current| {
		*current.borrow_mut() = Some(NotificationConnection {
			socket: ws,
			_on_open: on_open,
			_on_message: on_message,
			_on_close: on_close,
		});
	});
}

/// Ensure the notification WebSocket exists without forcing a reconnect.
#[cfg(wasm)]
pub fn ensure_notifications_connected() {
	NOTIFICATIONS_ENABLED.with(|enabled| enabled.set(true));
	let connected = CURRENT_WS.with(|prev| prev.borrow().is_some());
	if !connected {
		connect_notifications();
	}
}

/// Native builds render server-side HTML and do not open browser WebSockets.
#[cfg(not(wasm))]
pub fn ensure_notifications_connected() {}

/// End the authenticated connection and discard its reconnect and subscription state.
///
/// Call before leaving an authenticated session so a later login cannot inherit
/// the previous user's socket or queued subscriptions.
#[cfg(wasm)]
pub fn disconnect_notifications() {
	NOTIFICATIONS_ENABLED.with(|enabled| enabled.set(false));
	RECONNECT_TIMEOUT.with(|timer| timer.borrow_mut().take());
	CURRENT_WS.with(|current| current.borrow_mut().take());
	SUBSCRIBED_IDS.with(|ids| ids.borrow_mut().clear());
	APP_LOG_DEPLOYMENT_ID.with(|id| id.borrow_mut().take());
	RECONNECT_ATTEMPTS.with(|attempts| *attempts.borrow_mut() = 0);
}

/// Native rendering does not own a browser notification connection.
#[cfg(not(wasm))]
pub fn disconnect_notifications() {}

/// Record deployment IDs that should be re-subscribed after reconnect.
#[cfg(wasm)]
pub fn track_subscriptions(deployment_ids: &[String]) {
	SUBSCRIBED_IDS.with(|ids| {
		let mut ids = ids.borrow_mut();
		for id in deployment_ids {
			ids.insert(id.clone());
		}
	});
}

/// Subscribe the live app-log stream for the selected deployment.
#[cfg(wasm)]
pub fn subscribe_app_logs(deployment_id: &str) {
	let deployment_id = deployment_id.trim();
	if deployment_id.is_empty() {
		unsubscribe_logs();
		return;
	}

	APP_LOG_DEPLOYMENT_ID.with(|current| {
		*current.borrow_mut() = Some(deployment_id.to_string());
	});
	ensure_notifications_connected();
	CURRENT_WS.with(|current| {
		if let Some(connection) = current.borrow().as_ref()
			&& connection.socket.ready_state() == WebSocket::OPEN
		{
			send_client_message(
				&connection.socket,
				&WsClientMessage::SubscribeAppLogs {
					deployment_id: deployment_id.to_string(),
				},
			);
		}
	});
}

#[cfg(not(wasm))]
pub fn subscribe_app_logs(_deployment_id: &str) {}

/// Stop the active app-log stream.
#[cfg(wasm)]
pub fn unsubscribe_logs() {
	APP_LOG_DEPLOYMENT_ID.with(|current| {
		*current.borrow_mut() = None;
	});
	CURRENT_WS.with(|current| {
		if let Some(connection) = current.borrow().as_ref()
			&& connection.socket.ready_state() == WebSocket::OPEN
		{
			send_client_message(&connection.socket, &WsClientMessage::UnsubscribeLogs);
		}
	});
}

#[cfg(not(wasm))]
pub fn unsubscribe_logs() {}

#[cfg(wasm)]
fn send_client_message(ws: &WebSocket, message: &WsClientMessage) {
	if let Ok(json) = serde_json::to_string(message) {
		let _ = ws.send_with_str(&json);
	}
}

/// Dispatch a parsed server message to the appropriate UI handler.
#[cfg(wasm)]
fn handle_ws_message(msg: WsMessage) {
	match msg {
		WsMessage::DeploymentStatus(payload) => {
			update_deployment_badge(&payload);
			if matches!(
				payload.status,
				DeploymentState::Failed | DeploymentState::Degraded
			) {
				show_toast(
					&NotificationLevel::Warning,
					&payload.name,
					payload.message.as_deref().unwrap_or("Status changed"),
				);
			}
		}
		WsMessage::SystemNotification(payload) => {
			show_toast(&payload.level, &payload.title, &payload.message);
		}
		WsMessage::AppLog(payload) => log_viewer::append(payload),
		WsMessage::BuildLog(payload) => log_viewer::append_build(payload),
		WsMessage::ClusterHealth(payload) => cluster_health::update(payload),
		// LogStreamAck is surfaced via the connection layer; no DOM update
		// is required for it today.
		WsMessage::LogStreamAck(_) => {} // Exhaustive matching is intentional: adding a new `WsMessage`
		                                 // variant without handling it here will cause a compile-time error,
		                                 // ensuring no server messages are silently ignored on the client.
	}
}

/// Update a status badge element in the DOM for the given deployment.
#[cfg(wasm)]
fn update_deployment_badge(payload: &DeploymentStatusPayload) {
	let Some(document) = web_sys::window().and_then(|w| w.document()) else {
		return;
	};
	let selector = format!(
		"[data-deployment-id='{}'] .{}",
		payload.deployment_id,
		STYLES.status_badge().as_str(),
	);
	let Ok(Some(badge)) = document.query_selector(&selector) else {
		return;
	};

	let (color, label) = status_badge::badge_style(&payload.status);
	badge
		.set_attribute(
			"class",
			&format!("{} {color}", STYLES.status_badge().as_str()),
		)
		.unwrap();
	badge.set_text_content(Some(label));
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case::home("/", true)]
	#[case::clusters("/clusters", true)]
	#[case::deployments("/deployments", true)]
	#[case::github("/github", false)]
	#[case::login("/login", false)]
	#[case::register("/register", false)]
	#[case::unknown("/missing", false)]
	fn notification_connection_paths_match_authenticated_spa_routes(
		#[case] path: &str,
		#[case] expected: bool,
	) {
		// Arrange + Act
		let should_connect = should_connect_notifications_for_path(path);

		// Assert
		assert_eq!(should_connect, expected);
	}
}
