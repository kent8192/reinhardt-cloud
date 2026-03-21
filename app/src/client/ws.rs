//! WebSocket connection manager for the WASM client.
//!
//! Establishes a single global connection to `/ws/notifications`,
//! authenticates with JWT, and dispatches incoming messages to
//! toast notifications and status badge updates.

#[cfg(wasm)]
use std::cell::RefCell;
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
	DeploymentStatusPayload, NotificationLevel, WsClientMessage, WsMessage,
};

#[cfg(wasm)]
use super::components::status_badge;
#[cfg(wasm)]
use super::components::toast::show_toast;

#[cfg(wasm)]
thread_local! {
	static SUBSCRIBED_IDS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
	static RECONNECT_ATTEMPTS: RefCell<u32> = RefCell::new(0);
}

#[cfg(wasm)]
const MAX_RECONNECT_ATTEMPTS: u32 = 10;

/// Open a WebSocket to `/ws/notifications` and wire up event handlers.
///
/// On open the connection authenticates with the JWT stored in
/// `localStorage("auth_token")` and re-subscribes to any deployment
/// IDs previously registered via [`track_subscriptions`].
///
/// Automatically reconnects on close up to [`MAX_RECONNECT_ATTEMPTS`]
/// times with a fixed 3-second delay.
#[cfg(wasm)]
pub fn connect_notifications() {
	let Some(token) = get_auth_token() else {
		return;
	};

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

	// On open: reset reconnect counter, send JWT, re-subscribe
	let token_for_open = token.clone();
	let ws_for_open = ws.clone();
	let on_open = Closure::wrap(Box::new(move |_: web_sys::Event| {
		RECONNECT_ATTEMPTS.with(|c| *c.borrow_mut() = 0);

		let auth = WsClientMessage::Authenticate {
			token: token_for_open.clone(),
		};
		if let Ok(json) = serde_json::to_string(&auth) {
			let _ = ws_for_open.send_with_str(&json);
		}

		SUBSCRIBED_IDS.with(|ids| {
			let ids = ids.borrow();
			if !ids.is_empty() {
				let sub = WsClientMessage::Subscribe {
					deployment_ids: ids.iter().cloned().collect(),
				};
				if let Ok(json) = serde_json::to_string(&sub) {
					let _ = ws_for_open.send_with_str(&json);
				}
			}
		});
	}) as Box<dyn FnMut(_)>);
	ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
	on_open.forget();

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
	on_message.forget();

	// On close: auto-reconnect with attempt limit
	let on_close = Closure::wrap(Box::new(move |_: web_sys::Event| {
		let should_reconnect = RECONNECT_ATTEMPTS.with(|c| {
			let mut count = c.borrow_mut();
			*count += 1;
			*count <= MAX_RECONNECT_ATTEMPTS
		});
		if should_reconnect {
			gloo_timers::callback::Timeout::new(3_000, || {
				connect_notifications();
			})
			.forget();
		}
	}) as Box<dyn FnMut(_)>);
	ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
	on_close.forget();
}

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

/// Dispatch a parsed server message to the appropriate UI handler.
#[cfg(wasm)]
fn handle_ws_message(msg: WsMessage) {
	match msg {
		WsMessage::DeploymentStatus(payload) => {
			update_deployment_badge(&payload);
			if matches!(
				payload.status,
				crate::shared::ws_messages::DeploymentState::Failed
					| crate::shared::ws_messages::DeploymentState::Degraded
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
		WsMessage::AuthResult(payload) => {
			if !payload.success {
				show_toast(
					&NotificationLevel::Critical,
					"Authentication",
					payload
						.message
						.as_deref()
						.unwrap_or("WebSocket auth failed"),
				);
			}
		}
	}
}

/// Update a status badge element in the DOM for the given deployment.
#[cfg(wasm)]
fn update_deployment_badge(payload: &DeploymentStatusPayload) {
	let Some(document) = web_sys::window().and_then(|w| w.document()) else {
		return;
	};
	let selector = format!(
		"[data-deployment-id='{}'] .status-badge",
		payload.deployment_id
	);
	let Ok(Some(badge)) = document.query_selector(&selector) else {
		return;
	};

	let (color, label) = status_badge::badge_style(&payload.status);
	badge
		.set_attribute(
			"class",
			&format!(
				"status-badge inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium {color}"
			),
		)
		.unwrap();
	badge.set_text_content(Some(label));
}

/// Retrieve the JWT from `localStorage`.
#[cfg(wasm)]
fn get_auth_token() -> Option<String> {
	web_sys::window()?
		.local_storage()
		.ok()??
		.get_item("auth_token")
		.ok()?
}
