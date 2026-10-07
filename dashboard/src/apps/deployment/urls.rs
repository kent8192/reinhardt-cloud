//! Typed runtime operation routes owned by the Deployment App.
use reinhardt::UnifiedRouter;
#[cfg(server)]
use reinhardt::pages::server_fn::ServerFnRouterExt;
pub fn url_patterns() -> UnifiedRouter {
	UnifiedRouter::new()
		.server(|server| server.server_fn(super::functions::change_runtime::marker))
		.with_namespace("deployment")
}
