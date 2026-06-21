//! MSW-backed frontend E2E coverage for Dashboard browser flows.
//!
//! The test launches the real dashboard WASM client and intercepts
//! server-function fetches with Reinhardt's `MockServiceWorker`. This keeps
//! the test fast and deterministic while exercising the browser-rendered form
//! and `server_fn` request path.

use std::cell::Cell;
use std::rc::Rc;

use reinhardt::pages::ClientLauncher;
use reinhardt::test::fixtures::wasm::{msw_worker, screen, wasm_test_env};
use reinhardt::test::wasm::{UserEvent, wait_for};
use reinhardt_cloud_dashboard::apps::clusters::server_fn::{
	ClusterInfo, ClusterTokenInfo, create_cluster_for_current_org, list_clusters_for_current_org,
};
use reinhardt_cloud_dashboard::client::router::init_router;
use reinhardt_cloud_dashboard::shared::client::state;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;
use web_sys::HtmlInputElement;

wasm_bindgen_test_configure!(run_in_browser);

fn prepare_mount_point() {
	let window = web_sys::window().expect("window");
	let document = window.document().expect("document");
	let body = document.body().expect("body");

	if let Some(existing) = document.get_element_by_id("app") {
		existing.remove();
	}
	let app_div = document.create_element("div").expect("create #app div");
	app_div.set_id("app");
	body.append_child(&app_div).expect("append #app to body");
}

fn push_test_path(path: &str) {
	let window = web_sys::window().expect("window");
	window
		.history()
		.expect("history")
		.push_state_with_url(&JsValue::NULL, "", Some(path))
		.expect("push test path");
}

fn launch_dashboard_at(path: &str) {
	prepare_mount_point();
	push_test_path(path);
	ClientLauncher::new("#app")
		.before_launch(state::init_app_state)
		.router_client(init_router)
		.launch()
		.expect("ClientLauncher::launch must succeed");
}

fn cluster_fixture() -> ClusterInfo {
	ClusterInfo {
		id: 42,
		name: "prod-us-east".to_string(),
		api_url: "https://kubernetes.example.com:6443".to_string(),
		is_active: true,
		token_last_rotated_at: Some("2026-06-21T00:00:00Z".to_string()),
	}
}

#[wasm_bindgen_test]
async fn clusters_page_loads_and_submits_with_msw() {
	// Arrange
	let _env = wasm_test_env();
	let worker = msw_worker().await;
	let create_call_count = Rc::new(Cell::new(0));
	let create_call_count_for_handler = Rc::clone(&create_call_count);

	worker.handle_server_fn::<list_clusters_for_current_org::marker>(|_args| {
		Ok(vec![cluster_fixture()])
	});
	worker.handle_server_fn::<create_cluster_for_current_org::marker>(move |args| {
		create_call_count_for_handler.set(create_call_count_for_handler.get() + 1);
		assert_eq!(args.name, "staging-eu");
		assert_eq!(args.api_url, "https://staging.example.com:6443");
		Ok(ClusterTokenInfo {
			cluster: ClusterInfo {
				id: 43,
				name: args.name,
				api_url: args.api_url,
				is_active: true,
				token_last_rotated_at: Some("2026-06-21T00:01:00Z".to_string()),
			},
			auth_token: "rc-agent-token-e2e".to_string(),
		})
	});

	// Act
	launch_dashboard_at("/clusters");
	let screen = screen();

	// Assert
	let screen_for_inventory_wait = screen.clone();
	wait_for(move || {
		screen_for_inventory_wait
			.get_by_text("prod-us-east")
			.query()
			.is_some()
	})
	.with_description("cluster inventory rendered from MSW data")
	.await
	.expect("cluster inventory should render");
	worker
		.calls_to_server_fn::<list_clusters_for_current_org::marker>()
		.assert_called();

	let name_input: HtmlInputElement = screen
		.get_by_placeholder_text("prod-us-east")
		.get()
		.dyn_into()
		.expect("cluster name input");
	let api_url_input: HtmlInputElement = screen
		.get_by_placeholder_text("https://kubernetes.example.com:6443")
		.get()
		.dyn_into()
		.expect("cluster API URL input");
	let submit = screen
		.get_by_role_with_name("button", "Create cluster")
		.get();

	// Act
	UserEvent::type_text(&name_input, "staging-eu");
	UserEvent::type_text(&api_url_input, "https://staging.example.com:6443");
	UserEvent::click(&submit);

	// Assert
	let create_call_count_for_wait = Rc::clone(&create_call_count);
	wait_for(move || create_call_count_for_wait.get() == 1)
		.with_description("create cluster server_fn handled by MSW")
		.await
		.expect("create cluster server_fn should be called");
	worker
		.calls_to_server_fn::<create_cluster_for_current_org::marker>()
		.assert_count(1);
}
