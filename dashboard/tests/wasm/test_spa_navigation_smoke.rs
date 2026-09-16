//! SPA navigation smoke test — closes the structural test gap behind
//! the 7-iteration SPA regression chain (#4075 → #4088 → #4122 → #4203
//! → #4213 → #4217 → #4221).
//!
//! Mirrors upstream `kent8192/reinhardt-web` PR #4227's
//! `nav_diag_dom_advances_through_full_link_click_chain`, but launches
//! the dashboard's actual `init_router()` (named routes for
//! `dashboard:home`, `clusters:list`, etc.) so a regression in the
//! `UnifiedRouter::client(...) → ClientLauncher → reinhardt-pages
//! Router::push → reinhardt-urls push_state → state_to_js_object` chain
//! is caught by tests that sit on the actual click-to-render path. Every
//! framework-side fix in the regression chain has passed framework-side
//! tests because no test exercised the consumer-facing topology — this
//! test removes that gap.
//!
//! Refs `kent8192/reinhardt-cloud#574`.

use std::cell::Cell;
use std::rc::Rc;

use js_sys::Reflect;
use reinhardt::pages::ClientLauncher;
use reinhardt::pages::server_fn::ServerFnError;
use reinhardt::test::fixtures::wasm::msw_worker;
use reinhardt::test::wasm::{UserEvent, flush_effects, wait_for};
use reinhardt_cloud_dashboard::apps::auth::server_fn::me::me;
use reinhardt_cloud_dashboard::apps::clusters::server_fn::list_clusters_for_current_org;
use reinhardt_cloud_dashboard::client::router::init_router;
use reinhardt_cloud_dashboard::shared::UserInfo;
use reinhardt_cloud_dashboard::shared::client::state;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;
use web_sys::{HtmlElement, HtmlInputElement};

wasm_bindgen_test_configure!(run_in_browser);

/// Ensure the SPA mount point exists on the test page. `wasm-pack test`
/// produces a bare HTML wrapper without `<div id="app">`, so we provision
/// one before launching the client.
fn ensure_mount_point(document: &web_sys::Document, body: &web_sys::HtmlElement) {
	if document.get_element_by_id("app").is_some() {
		return;
	}
	let app_div = document.create_element("div").expect("create #app div");
	app_div.set_id("app");
	body.append_child(&app_div).expect("append #app to body");
}

/// Launch the dashboard's client just like `dashboard/src/client.rs`
/// does in production, minus the notification WebSocket bootstrap and
/// `on_path` hooks that depend on the deployed dashboard runtime. Reuses
/// the same `init_router()` and the same `before_launch(state::init_app_state)`
/// hook so route names, guards, and launcher topology match production.
/// Client-side route reversal reads the launcher's active route tree.
fn launch_dashboard_for_test() {
	ClientLauncher::new("#app")
		.before_launch(state::init_app_state)
		.router_client(init_router)
		.launch()
		.expect("ClientLauncher::launch must succeed");
}

#[wasm_bindgen_test]
async fn link_click_preserves_route_state_and_bound_auth_inputs() {
	// Arrange — mirror the dashboard's launch path
	let worker = msw_worker().await;
	let authenticated = Rc::new(Cell::new(false));
	let auth_checks = Rc::new(Cell::new(0));
	let authenticated_for_handler = Rc::clone(&authenticated);
	let auth_checks_for_handler = Rc::clone(&auth_checks);
	worker.handle_server_fn::<me::marker>(move |_| {
		auth_checks_for_handler.set(auth_checks_for_handler.get() + 1);
		if !authenticated_for_handler.get() {
			return Err(ServerFnError::auth(401, "Sign in required"));
		}
		Ok(UserInfo {
			id: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
			username: "alice".to_owned(),
			email: "alice@example.com".to_owned(),
		})
	});
	worker.handle_server_fn::<list_clusters_for_current_org::marker>(|_| Ok(Vec::new()));
	let window = web_sys::window().expect("window");
	window
		.history()
		.expect("history")
		.replace_state_with_url(&JsValue::NULL, "", Some("/login"))
		.expect("start at a public route");
	let document = window.document().expect("document");
	let body = document.body().expect("body");
	ensure_mount_point(&document, &body);
	launch_dashboard_for_test();

	let anchor: HtmlElement = document
		.create_element("a")
		.expect("create anchor")
		.dyn_into()
		.expect("anchor as HtmlElement");
	anchor.set_attribute("href", "/clusters").expect("set href");
	body.append_child(&anchor).expect("append anchor to body");

	// Act — an anonymous navigation must stop before the protected child mounts.
	anchor.click();
	let checks_for_wait = Rc::clone(&auth_checks);
	wait_for(move || checks_for_wait.get() > 0)
		.with_description("authentication guard evaluated")
		.await
		.expect("authentication guard should run");
	flush_effects().await;

	// Assert
	assert_eq!(window.location().pathname().expect("pathname"), "/login");
	assert_eq!(document.get_element_by_id("create-cluster-name"), None);
	worker
		.calls_to_server_fn::<list_clusters_for_current_org::marker>()
		.assert_count(0);
	authenticated.set(true);

	// Act — bubbling click that the framework's link interceptor must
	// observe and route through `Router::push → Router::navigate →
	// push_state → state_to_js_object`
	anchor.click();
	wait_for(|| {
		web_sys::window()
			.expect("window")
			.location()
			.pathname()
			.expect("pathname")
			== "/clusters"
	})
	.with_description("authenticated navigation committed")
	.await
	.expect("authenticated navigation should commit");

	// Assert — exact symptom predicates from #4221 round 7
	let history = window.history().expect("history");
	let state: JsValue = history.state().expect("history.state");

	assert!(
		!state.is_string(),
		"history.state must be a JS object (not a JSON string); \
		 #4221 round 7 invariant. Saw state.is_string() == true. \
		 Raw value: {:?}",
		state.as_string().unwrap_or_default()
	);
	assert!(
		state.is_object(),
		"history.state must be a JS object; #4218 invariant"
	);

	let route_name = Reflect::get(&state, &JsValue::from_str("route_name"))
		.expect("Reflect::get(state, 'route_name')")
		.as_string()
		.unwrap_or_default();
	assert_eq!(
		route_name, "clusters:list",
		"named route must resolve to 'clusters:list'; \
		 empty route_name was the original #4221 symptom"
	);

	let path = Reflect::get(&state, &JsValue::from_str("path"))
		.expect("Reflect::get(state, 'path')")
		.as_string()
		.unwrap_or_default();
	assert_eq!(
		path, "/clusters",
		"history.state.path must reflect the navigation target"
	);

	for (path, inputs) in [
		(
			"/login",
			&[
				("login-username", "alice"),
				("login-password", "correct-horse-battery-staple"),
			][..],
		),
		(
			"/register",
			&[
				("register-username", "alice"),
				("register-email", "alice@example.com"),
				("register-password", "correct-horse-battery-staple"),
			][..],
		),
	] {
		// Arrange
		anchor.set_attribute("href", path).expect("set auth route");
		anchor.click();
		let document_for_wait = document.clone();
		let first_input_id = inputs[0].0;
		wait_for(move || {
			document_for_wait
				.get_element_by_id(first_input_id)
				.is_some()
		})
		.with_description("auth route inputs rendered")
		.await
		.expect("auth route should render");

		for &(id, value) in inputs {
			let input: HtmlInputElement = document
				.get_element_by_id(id)
				.expect("auth input")
				.dyn_into()
				.expect("auth input element");

			// Act
			UserEvent::type_text(&input, value);
			flush_effects().await;

			// Assert
			assert_eq!(
				document.get_element_by_id(id).as_ref(),
				Some(input.as_ref())
			);
			assert_eq!(document.active_element().as_ref(), Some(input.as_ref()));
			assert_eq!(input.value(), value);
			if input.type_() == "password" {
				assert_eq!(input.get_attribute("value"), None);
			}
		}
	}
}
