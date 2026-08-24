//! SPA router configuration for the Reinhardt Cloud dashboard.
//!
//! The dashboard owns one route tree. Public pages live at the root, while
//! authenticated pages share the `dashboard_layout` outlet.

use reinhardt::pages::router::ClientRouter;

use crate::apps::auth::client::pages::{account_page, login_page, register_page};
use crate::apps::clusters::client::pages::clusters_list_page;
use crate::apps::dashboard::client::layout::{dashboard_layout, dashboard_shell};
use crate::apps::deployments::client::pages::deployments_list_page;
use crate::apps::github::client::pages::github_repositories_page;
use crate::shared::client::pages::not_found::not_found_page;

/// Add the dashboard's complete route tree to an existing client router.
///
/// The native project router and the WASM launcher both use this function, so
/// named route reversal and layout nesting stay identical on both targets.
pub(crate) fn configure_routes(router: ClientRouter) -> ClientRouter {
	router.not_found(not_found_page).routes(|routes| {
		routes
			.component(login_page)
			.component(register_page)
			.layout(dashboard_layout, |children| {
				children
					.index(dashboard_shell)
					.component(account_page)
					.component(clusters_list_page)
					.component(deployments_list_page)
					.component(github_repositories_page)
			})
	})
}

/// Build the dashboard SPA router.
///
/// `ClientLauncher::router_client(init_router)` owns this router on WASM;
/// server routes are built separately in `crate::config::urls`.
pub fn init_router() -> ClientRouter {
	configure_routes(ClientRouter::new())
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use super::*;

	#[rstest]
	#[case::home("dashboard:home", "/")]
	#[case::account("auth:account_page", "/account")]
	#[case::login("auth:login_page", "/login")]
	#[case::register("auth:register_page", "/register")]
	#[case::clusters("clusters:list", "/clusters")]
	#[case::deployments("deployments:list", "/deployments")]
	#[case::github("github:repositories", "/github")]
	fn routes_preserve_public_paths(#[case] name: &str, #[case] expected: &str) {
		// Arrange
		let router = configure_routes(ClientRouter::new());

		// Act
		let path = router.reverse(name, &[]);

		// Assert
		assert_eq!(path, Ok(expected.to_string()));
	}

	#[rstest]
	#[case::home("/")]
	#[case::account("/account")]
	#[case::clusters("/clusters")]
	#[case::deployments("/deployments")]
	#[case::github("/github")]
	fn authenticated_routes_share_dashboard_layout(#[case] path: &str) {
		// Arrange
		let router = configure_routes(ClientRouter::new());

		// Act
		let matched = router.match_tree(path);

		// Assert
		assert_eq!(matched.map(|tree| tree.layouts().len()), Some(1));
	}
}
