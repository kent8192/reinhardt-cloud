//! Target-neutral routes owned by this App.

use reinhardt::{UnifiedRouter, url_patterns};

#[url_patterns]
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|server| {
			server
				.endpoint(super::server::sign_in_page)
				.endpoint(super::server::login)
				.endpoint(super::server::logout)
		})
		.client(|_| client_url_patterns())
		.with_namespace("identity")
}

pub fn client_url_patterns() -> reinhardt::ClientRouter {
	reinhardt::ClientRouter::new().component(super::client::components::sign_in::sign_in)
}
