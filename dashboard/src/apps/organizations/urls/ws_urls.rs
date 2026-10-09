//! WebSocket routes for the organizations application.
//!
//! WebSocket handlers register with their final absolute paths; the app router
//! is mounted at `/`.

use reinhardt::WebSocketRouter;

/// Return the WebSocket routes contributed by this application.
pub fn ws_url_patterns() -> WebSocketRouter {
	WebSocketRouter::new()
}
