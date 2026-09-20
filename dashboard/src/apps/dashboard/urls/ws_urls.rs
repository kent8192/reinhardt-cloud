//! WebSocket URL configuration for the dashboard app.
//!
//! Registers the dashboard notification consumer used by the SPA.

use reinhardt::WebSocketRouter;

use crate::utils::realtime::consumer::NotificationConsumerEndpoint;

/// Returns the WebSocket URL patterns for dashboard endpoints.
pub fn ws_url_patterns() -> WebSocketRouter {
	WebSocketRouter::new().consumer(|| NotificationConsumerEndpoint)
}
