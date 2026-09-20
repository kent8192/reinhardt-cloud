//! Browser regression coverage for notification ownership across SPA sessions.

use reinhardt::test::fixtures::wasm::wasm_test_env;
use reinhardt_cloud_dashboard::shared::client::ws;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen(inline_js = r#"
let originalWebSocket;
let sockets;
export function installNotificationSockets() {
    originalWebSocket = globalThis.WebSocket;
    sockets = [];
    globalThis.WebSocket = class {
        constructor(url) {
            this.url = url;
            this.readyState = 0;
            this.messages = [];
            this.closeCalls = 0;
            sockets.push(this);
        }
        send(message) { this.messages.push(message); }
        close() { this.closeCalls += 1; this.readyState = 3; }
    };
}
export function restoreNotificationSockets() { globalThis.WebSocket = originalWebSocket; }
export function openNotificationSocket() {
    const socket = sockets.at(-1);
    socket.readyState = 1;
    socket.onopen?.(new Event('open'));
}
export function closeNotificationSocketFromServer() {
    const socket = sockets.at(-1);
    socket.readyState = 3;
    socket.onclose?.(new Event('close'));
}
export function notificationSocketCount() { return sockets.length; }
export function notificationMessageCount() { return sockets.at(-1).messages.length; }
export function notificationSocketDisposed() {
    const socket = sockets.at(-1);
    return socket.readyState === 3 && socket.closeCalls === 1 &&
        socket.onopen == null && socket.onmessage == null && socket.onclose == null;
}
export function waitNotificationReconnect() { return new Promise(resolve => setTimeout(resolve, 3200)); }
"#)]
extern "C" {
	fn installNotificationSockets();
	fn restoreNotificationSockets();
	fn openNotificationSocket();
	fn closeNotificationSocketFromServer();
	fn notificationSocketCount() -> u32;
	fn notificationMessageCount() -> u32;
	pub(super) fn notificationSocketDisposed() -> bool;
	fn waitNotificationReconnect() -> js_sys::Promise;
}

pub(super) struct NotificationSocketFixture;

impl NotificationSocketFixture {
	pub(super) fn new() -> Self {
		ws::disconnect_notifications();
		installNotificationSockets();
		Self
	}
}

impl Drop for NotificationSocketFixture {
	fn drop(&mut self) {
		ws::disconnect_notifications();
		restoreNotificationSockets();
	}
}

#[rstest::rstest]
#[test_attr(wasm_bindgen_test)]
async fn ending_a_session_cancels_the_pending_notification_reconnect() {
	// Arrange
	let _env = wasm_test_env();
	let _sockets = NotificationSocketFixture::new();
	ws::ensure_notifications_connected();
	openNotificationSocket();
	closeNotificationSocketFromServer();

	// Act
	ws::disconnect_notifications();
	JsFuture::from(waitNotificationReconnect())
		.await
		.expect("reconnect deadline");

	// Assert
	assert_eq!(notificationSocketCount(), 1);
	assert_eq!(notificationSocketDisposed(), true);
}

#[rstest::rstest]
#[test_attr(wasm_bindgen_test)]
fn a_new_session_does_not_inherit_notification_subscriptions() {
	// Arrange
	let _env = wasm_test_env();
	let _sockets = NotificationSocketFixture::new();
	ws::track_subscriptions(&["previous-deployment".to_owned()]);
	ws::subscribe_app_logs("previous-deployment");
	openNotificationSocket();
	assert_eq!(notificationMessageCount(), 2);

	// Act
	ws::disconnect_notifications();
	assert_eq!(notificationSocketDisposed(), true);
	ws::ensure_notifications_connected();
	openNotificationSocket();

	// Assert
	assert_eq!(notificationSocketCount(), 2);
	assert_eq!(notificationMessageCount(), 0);
}
