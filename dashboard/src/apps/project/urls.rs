//! Target-neutral routes owned by this App.

use crate::apps::project::client::components::projects::projects;
use reinhardt::{ClientRouter, UnifiedRouter, url_patterns};

#[url_patterns]
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|server| {
			server
				.endpoint(crate::apps::project::server::index)
				.endpoint(crate::apps::project::server::list)
				.endpoint(crate::apps::project::server::detail)
		})
		.client(|_| client_url_patterns())
		.with_namespace("project")
}

pub fn client_url_patterns() -> ClientRouter {
	ClientRouter::new().component(projects)
}
