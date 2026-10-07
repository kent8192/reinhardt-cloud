//! Target-neutral routes owned by this App.
#[cfg(server)]
use reinhardt::pages::server_fn::ServerFnRouterExt;

use crate::apps::project::client::components::projects::projects;
use reinhardt::{ClientRouter, UnifiedRouter, url_patterns};

#[url_patterns]
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|server| {
			server
				.endpoint(crate::apps::project::server::index)
				.endpoint(crate::apps::project::server::project_page)
				.endpoint(crate::apps::project::server::projects_page)
				.server_fn(super::functions::load_projects::marker)
				.server_fn(super::functions::load_project::marker)
				.endpoint(crate::apps::project::server::list)
				.endpoint(crate::apps::project::server::detail)
		})
		.client(|_| client_url_patterns())
		.with_namespace("project")
}

pub fn client_url_patterns() -> ClientRouter {
	ClientRouter::new().routes(|routes| {
		routes.layout(
			super::client::components::layout::organization_shell,
			|children| {
				children
					.component(projects)
					.component(super::client::components::detail::project)
			},
		)
	})
}
