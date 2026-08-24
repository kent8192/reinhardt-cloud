//! URL configuration for the dashboard app.
//!
//! Declares server-side routes for the dashboard app.
//!
//! The dashboard SPA tree is registered centrally by `crate::client::router`
//! so all authenticated pages share one nested layout.

pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(native)]
use reinhardt::pages::router::ClientRouter;

#[cfg(native)]
type AppRouter = UnifiedRouter<ClientRouter>;
#[cfg(not(native))]
type AppRouter = UnifiedRouter;

/// Returns the unified URL patterns for the dashboard app.
pub fn url_patterns() -> AppRouter {
	UnifiedRouter::new().server(|s| s).client(|c| c)
}
