//! Target-neutral routes owned by this App.

#[cfg(server)]
use crate::apps::observability::server::healthz;
use reinhardt::{UnifiedRouter, url_patterns};

#[url_patterns]
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new().server(|server| server.endpoint(healthz))
}
