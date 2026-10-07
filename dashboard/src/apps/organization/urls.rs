//! Target-neutral routes owned by this App.
#[cfg(server)]
use reinhardt::pages::server_fn::ServerFnRouterExt;

use reinhardt::{UnifiedRouter, url_patterns};

#[url_patterns]
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|server| {
			server
				.endpoint(super::server::list)
				.endpoint(super::server::chooser_page)
				.server_fn(super::functions::load_organizations::marker)
		})
		.client(|_| client_url_patterns())
		.with_namespace("organization")
}

pub fn client_url_patterns() -> reinhardt::ClientRouter {
	reinhardt::ClientRouter::new().component(super::client::components::chooser::organizations)
}
