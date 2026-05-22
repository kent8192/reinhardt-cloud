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

use js_sys::Reflect;
use reinhardt::pages::ClientLauncher;
use reinhardt_cloud_dashboard::client::router::init_router;
use reinhardt_cloud_dashboard::shared::client::state;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;
use web_sys::HtmlElement;

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
/// does in production, minus the `on_path` hooks that depend on a live
/// notifications WebSocket. Reuses the same `init_router()` and the same
/// `before_launch(state::init_app_state)` hook so route names, patterns,
/// and launcher topology match production byte-for-byte —
/// `UnifiedRouter::register_globally()` inside `init_router` installs
/// the `ClientUrlReverser` that `ResolvedUrls::from_global()` consumes.
fn launch_dashboard_for_test() {
	ClientLauncher::new("#app")
		.before_launch(state::init_app_state)
		.router_client(init_router)
		.launch()
		.expect("ClientLauncher::launch must succeed");
}

#[wasm_bindgen_test]
fn link_click_advances_history_state_as_object() {
	// Arrange — mirror the dashboard's launch path
	let window = web_sys::window().expect("window");
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

	// Act — bubbling click that the framework's link interceptor must
	// observe and route through `Router::push → Router::navigate →
	// push_state → state_to_js_object`
	anchor.click();

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
}
