//! Target-neutral routes owned by this App.

use reinhardt::{UnifiedRouter, url_patterns};

#[url_patterns]
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|server| {
			server
				.endpoint(super::server::login)
				.endpoint(super::server::logout)
		})
		.with_namespace("identity")
}
