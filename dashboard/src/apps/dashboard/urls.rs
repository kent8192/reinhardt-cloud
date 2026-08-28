//! URL configuration for the dashboard app.
//!
//! Declares server-side routes for the dashboard app.
//!
//! The dashboard SPA tree is registered centrally by `crate::client::router`
//! so all authenticated pages share one nested layout.

#[cfg(server)]
pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(server)]
use reinhardt::pages::router::ClientRouter;

#[cfg(server)]
type AppRouter = UnifiedRouter<ClientRouter>;
#[cfg(not(server))]
type AppRouter = UnifiedRouter;

/// Returns the unified URL patterns for the dashboard app.
#[cfg(server)]
pub fn url_patterns() -> AppRouter {
	UnifiedRouter::new()
		.client(|client| client)
		.websocket(|websocket| websocket.mount("/", ws_urls::ws_url_patterns()))
}

#[cfg(not(server))]
pub fn url_patterns() -> AppRouter {
	UnifiedRouter::new()
}
