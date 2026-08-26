//! URL configuration for the health app.

#[cfg(server)]
pub mod ws_urls;

use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(server)]
use reinhardt::pages::router::ClientRouter;

#[cfg(server)]
type AppRouter = UnifiedRouter<ClientRouter>;
#[cfg(not(server))]
type AppRouter = UnifiedRouter;

#[cfg(server)]
use crate::apps::health::server_urls;

/// Returns the unified URL patterns for the health app.
///
/// The health app currently exposes only a server-side liveness probe;
/// the empty `.client(|c| c)` block keeps the composition pattern
/// uniform across all apps.
#[cfg(server)]
pub fn url_patterns() -> AppRouter {
	UnifiedRouter::new()
		.server(|server| server.endpoint(server_urls::healthz))
		.client(|client| client)
}

#[cfg(not(server))]
pub fn url_patterns() -> AppRouter {
	UnifiedRouter::new()
}
