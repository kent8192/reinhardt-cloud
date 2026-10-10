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
/// The WASM launcher executes this declaration. Native tests construct it
/// explicitly; the native project router only type-checks its client builder.
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

	use reinhardt::pages::NavigationGuard;
	use reinhardt::pages::reactive::ReactiveScope;

	use crate::apps::dashboard::client::layout::require_dashboard_session;

	use super::{ClientRouter, configure_routes};

	#[rstest]
	#[case::home("dashboard:home", "/")]
	#[case::account("auth:account_page", "/account")]
	#[case::login("auth:login_page", "/login")]
	#[case::register("auth:register_page", "/register")]
	#[case::clusters("clusters:list", "/clusters")]
	#[case::deployments("deployments:list", "/deployments")]
	#[case::github("github:repositories", "/github")]
	fn routes_preserve_public_paths(#[case] name: &str, #[case] expected: &str) {
		ReactiveScope::run(|| {
			// Arrange
			let router = configure_routes(ClientRouter::new());

			// Act
			let path = router.reverse(name, &[]);

			// Assert
			assert_eq!(path, Ok(expected.to_string()));
		});
	}

	#[rstest]
	#[case::home("/")]
	#[case::account("/account")]
	#[case::clusters("/clusters")]
	#[case::deployments("/deployments")]
	#[case::github("/github")]
	fn authenticated_routes_share_dashboard_layout(#[case] path: &str) {
		ReactiveScope::run(|| {
			// Arrange
			let router = configure_routes(ClientRouter::new());

			// Act
			let matched = router.match_tree(path);

			// Assert
			let matched = matched.expect("protected route is registered");
			assert_eq!(matched.layouts().len(), 1);
			assert_eq!(
				matched.navigation_guard_ids(),
				[require_dashboard_session::marker::ID],
			);
		});
	}

	#[rstest]
	#[case::login("/login")]
	#[case::register("/register")]
	fn public_routes_do_not_require_a_dashboard_session(#[case] path: &str) {
		ReactiveScope::run(|| {
			// Arrange
			let router = configure_routes(ClientRouter::new());

			// Act
			let matched = router.match_tree(path).expect("public route is registered");

			// Assert
			assert_eq!(matched.navigation_guard_ids(), []);
		});
	}
	#[cfg(native)]
	#[rstest]
	#[tokio::test]
	async fn native_protected_route_requires_a_browser_session() {
		// Arrange
		let scope = ReactiveScope::new();
		let router = scope.enter(super::init_router);
		let mut renderer = reinhardt::pages::SsrRenderer::new();

		// Act
		let output = renderer.render_route_to_string(&router, "/clusters").await;

		// Assert
		assert_eq!(output.status, 302);
		assert_eq!(output.html, "");
		assert_eq!(renderer.route_redirect_location(), Some("/login"));
	}
}
