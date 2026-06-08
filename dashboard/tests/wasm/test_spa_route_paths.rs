//! Client SPA route reverse-resolution coverage.
//!
//! v0.2.0-rc.2 removed the native typed client URL accessors. The SPA
//! routes are now declared in
//! [`crate::client::router::init_router`] (wasm-only) and are reversible there
//! through the merged [`ClientRouter`] under their `<app>:<route>` names.
//!
//! These tests assert each named SPA route reverses to its expected path so an
//! accidental path change (or a regression in `init_router` registration) is
//! caught at the unit layer, complementing the click-through coverage in
//! `test_spa_navigation_smoke`. This is the v0.2.0 home for the coverage that
//! previously lived in the native SPA path test module.
//!
//! `init_router()` registers the client routes with fully-qualified names, so
//! `clusters:list` and `deployments:list` do not collide (a bare `list` would).

use reinhardt_cloud_dashboard::client::router::init_router;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn spa_routes_reverse_to_expected_paths() {
	// Arrange — build the dashboard's real client SPA router.
	let router = init_router();

	// Act + Assert — each named SPA route reverses to its declared path.
	assert_eq!(
		router
			.reverse("auth:account_page", &[])
			.expect("auth:account_page must be reversible"),
		"/account"
	);
	assert_eq!(
		router
			.reverse("auth:login_page", &[])
			.expect("auth:login_page must be reversible"),
		"/login"
	);
	assert_eq!(
		router
			.reverse("auth:register_page", &[])
			.expect("auth:register_page must be reversible"),
		"/register"
	);
	assert_eq!(
		router
			.reverse("dashboard:home", &[])
			.expect("dashboard:home must be reversible"),
		"/"
	);
	assert_eq!(
		router
			.reverse("clusters:list", &[])
			.expect("clusters:list must be reversible"),
		"/clusters"
	);
	assert_eq!(
		router
			.reverse("deployments:list", &[])
			.expect("deployments:list must be reversible"),
		"/deployments"
	);
}
