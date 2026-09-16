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
use reinhardt_cloud_dashboard::apps::auth::server_fn::login::login;
use reinhardt_cloud_dashboard::apps::auth::server_fn::me::me;
use reinhardt_cloud_dashboard::apps::auth::server_fn::oauth_providers::list_oauth_providers;
use reinhardt_cloud_dashboard::apps::clusters::server_fn::{
	ClusterInfo, ClusterTokenInfo, create_cluster_for_current_org, list_clusters_for_current_org,
};
use reinhardt_cloud_dashboard::apps::dashboard::client::style::STYLES;
use reinhardt_cloud_dashboard::apps::deployments::server_fn::{
	DeploymentInfo, list_deployments_for_current_org,
};
use reinhardt_cloud_dashboard::client::router::init_router;
use reinhardt_cloud_dashboard::shared::client::state;
use reinhardt_cloud_dashboard::shared::{AuthResponse, UserInfo};
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;
use web_sys::{HtmlFormElement, HtmlInputElement};

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
		name: "query-loaded-cluster".to_string(),
		api_url: "https://kubernetes.example.com:6443".to_string(),
		is_active: true,
		token_last_rotated_at: Some("2026-06-21T00:00:00Z".to_string()),
	}
}

#[rstest::rstest]
#[test_attr(wasm_bindgen_test)]
async fn login_navigates_to_overview_with_current_organization_counts() {
	// Arrange
	let _env = wasm_test_env();
	let worker = msw_worker().await;
	worker.handle_server_fn::<list_oauth_providers::marker>(|_| Ok(vec![]));
	worker.handle_server_fn::<login::marker>(|args| {
		assert_eq!(args.request.username, "alice");
		assert_eq!(args.request.password, "example-password");
		Ok(AuthResponse {
			success: true,
			user: None,
		})
	});
	worker.handle_server_fn::<me::marker>(|_| {
		Ok(UserInfo {
			id: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
			username: "alice".to_owned(),
			email: "alice@example.com".to_owned(),
		})
	});
	worker.handle_server_fn::<list_clusters_for_current_org::marker>(|_| {
		Ok(vec![
			cluster_fixture(),
			ClusterInfo {
				id: 43,
				..cluster_fixture()
			},
		])
	});
	worker.handle_server_fn::<list_deployments_for_current_org::marker>(|_| {
		Ok(vec![DeploymentInfo {
			id: 7,
			project_name: "web".to_owned(),
			cluster_id: 42,
			status: "running".to_owned(),
			image: "registry.example.com/web:v1".to_owned(),
		}])
	});
	let document = web_sys::window()
		.expect("window")
		.document()
		.expect("document");
	let selector = format!(".{}", STYLES.metric_value().as_str());

	// Act
	launch_dashboard_at("/login");
	let screen = screen();
	let username: HtmlInputElement = screen
		.get_by_label_text("Username")
		.get()
		.dyn_into()
		.expect("username");
	let password: HtmlInputElement = screen
		.get_by_label_text("Password")
		.get()
		.dyn_into()
		.expect("password");
	UserEvent::type_text(&username, "alice");
	UserEvent::type_text(&password, "example-password");
	let form: HtmlFormElement = screen
		.get_by_role_with_name("button", "Sign in")
		.get()
		.parent_element()
		.expect("login form")
		.dyn_into()
		.expect("form element");
	form.request_submit().expect("submit login form");
	let document_for_wait = document.clone();
	let selector_for_wait = selector.clone();
	wait_for(move || {
		let metrics = document_for_wait
			.query_selector_all(&selector_for_wait)
			.expect("metrics");
		metrics.length() == 2
			&& metrics
				.item(0)
				.and_then(|node| node.text_content())
				.as_deref() == Some("2")
			&& metrics
				.item(1)
				.and_then(|node| node.text_content())
				.as_deref() == Some("1")
			&& metrics
				.item(0)
				.and_then(|node| node.parent_element())
				.is_some_and(|card| card.closest("[hidden]").expect("hidden ancestor").is_none())
	})
	.with_description("visible overview counts loaded from organization queries")
	.await
	.expect("overview should show actual resource counts");

	// Assert
	worker.calls_to_server_fn::<login::marker>().assert_called();
	worker
		.calls_to_server_fn::<list_clusters_for_current_org::marker>()
		.assert_called();
	worker
		.calls_to_server_fn::<list_deployments_for_current_org::marker>()
		.assert_called();
	let headings = document
		.query_selector_all("h3")
		.expect("overview headings");
	let labels = (0..headings.length())
		.map(|index| {
			headings
				.item(index)
				.expect("heading")
				.text_content()
				.unwrap_or_default()
		})
		.collect::<Vec<_>>();
	assert_eq!(labels, ["Clusters", "Deployments"]);
	assert_eq!(screen.get_by_text("Healthy").query().is_some(), false);
	assert_eq!(
		web_sys::window()
			.expect("window")
			.location()
			.pathname()
			.expect("pathname"),
		"/"
	);
}

#[wasm_bindgen_test]
async fn clusters_page_loads_and_submits_with_msw() {
	// Arrange
	let _env = wasm_test_env();
	let worker = msw_worker().await;
	let create_call_count = Rc::new(Cell::new(0));
	let create_call_count_for_handler = Rc::clone(&create_call_count);
	worker.handle_server_fn::<me::marker>(|_| {
		Ok(UserInfo {
			id: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
			username: "alice".to_owned(),
			email: "alice@example.com".to_owned(),
		})
	});

	worker.handle_server_fn::<list_clusters_for_current_org::marker>(|_args| {
		Ok(vec![cluster_fixture()])
	});
	worker.handle_server_fn::<create_cluster_for_current_org::marker>(move |args| {
		create_call_count_for_handler.set(create_call_count_for_handler.get() + 1);
		let name = args
			.payload
			.name()
			.expect("generated form supplies name")
			.to_string();
		let api_url = args
			.payload
			.api_url()
			.expect("generated form supplies API URL")
			.to_string();
		assert_eq!(name, "staging-eu");
		assert_eq!(api_url, "https://staging.example.com:6443");
		Ok(ClusterTokenInfo {
			cluster: ClusterInfo {
				id: 43,
				name,
				api_url,
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
			.get_by_text("query-loaded-cluster")
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
		.get_by_label_text("Name")
		.get()
		.dyn_into()
		.expect("cluster name input");

	let api_url_input: HtmlInputElement = screen
		.get_by_label_text("API URL")
		.get()
		.dyn_into()
		.expect("cluster API URL input");
	let name_for_visibility = name_input.clone();
	wait_for(move || {
		name_for_visibility
			.closest("[hidden]")
			.expect("hidden ancestor")
			.is_none()
	})
	.with_description("authenticated dashboard shell is visible")
	.await
	.expect("session verification should reveal the cluster form");

	// Act
	name_input.focus().expect("focus cluster name");
	UserEvent::type_text(&name_input, "staging-eu");
	assert!(
		name_input.is_same_node(
			screen
				.get_by_label_text("Name")
				.query()
				.as_ref()
				.map(|element| element.as_ref())
		)
	);
	assert!(
		name_input.is_same_node(
			web_sys::window()
				.unwrap()
				.document()
				.unwrap()
				.active_element()
				.as_ref()
				.map(|element| element.as_ref())
		)
	);
	api_url_input.focus().expect("focus cluster API URL");
	UserEvent::type_text(&api_url_input, "not-a-url");
	assert!(
		api_url_input.is_same_node(
			screen
				.get_by_label_text("API URL")
				.query()
				.as_ref()
				.map(|element| element.as_ref())
		)
	);

	let submit = screen
		.get_by_role_with_name("button", "Register cluster")
		.get();
	let create_form: HtmlFormElement = submit
		.parent_element()
		.expect("cluster create form parent")
		.dyn_into()
		.expect("cluster create form");
	assert_eq!(api_url_input.type_(), "url");
	assert!(!api_url_input.check_validity());
	create_form
		.request_submit()
		.expect("attempt invalid URL submit");
	assert_eq!(create_call_count.get(), 0);
	UserEvent::type_text(&api_url_input, "https://staging.example.com:6443");
	assert!(api_url_input.check_validity());
	create_form
		.request_submit()
		.expect("cluster create form should dispatch a submit event");

	// Assert
	let create_call_count_for_wait = Rc::clone(&create_call_count);
	wait_for(move || create_call_count_for_wait.get() == 1)
		.with_description("create cluster server_fn handled by MSW")
		.await
		.expect("create cluster server_fn should be called");
	worker
		.calls_to_server_fn::<create_cluster_for_current_org::marker>()
		.assert_count(1);
	let screen_for_reset = screen.clone();
	wait_for(move || {
		["Name", "API URL"].iter().all(|label| {
			screen_for_reset
				.get_by_label_text(label)
				.query()
				.and_then(|element| element.dyn_into::<HtmlInputElement>().ok())
				.is_some_and(|input| input.value().is_empty())
		})
	})
	.with_description("generated form reset synchronizes bound controls")
	.await
	.expect("cluster form should reset after successful mutation");
	let name_after_reset: HtmlInputElement = screen
		.get_by_label_text("Name")
		.get()
		.dyn_into()
		.expect("cluster name input after reset");
	let api_url_after_reset: HtmlInputElement = screen
		.get_by_label_text("API URL")
		.get()
		.dyn_into()
		.expect("cluster API URL input after reset");
	assert_eq!(name_after_reset.value(), "");
	assert_eq!(api_url_after_reset.value(), "");
}

#[rstest::rstest]
#[test_attr(wasm_bindgen_test)]
async fn registration_submits_normalized_text_without_changing_the_password() {
	// Arrange
	let _env = wasm_test_env();
	let worker = msw_worker().await;
	worker.handle_server_fn::<list_oauth_providers::marker>(|_| Ok(vec![]));
	worker
		.handle_server_fn::<reinhardt_cloud_dashboard::apps::auth::server_fn::register::register::marker>(
			|args| {
				assert_eq!(args.request.username, "alice");
				assert_eq!(args.request.email, "alice@example.com");
				assert_eq!(args.request.password, "  secret password  ");
				Ok(AuthResponse {
					success: true,
					user: None,
				})
			},
		);
	launch_dashboard_at("/register");
	let screen = screen();
	let username: HtmlInputElement = screen
		.get_by_label_text("Username")
		.get()
		.dyn_into()
		.unwrap();
	let email: HtmlInputElement = screen.get_by_label_text("Email").get().dyn_into().unwrap();
	let password: HtmlInputElement = screen
		.get_by_label_text("Password")
		.get()
		.dyn_into()
		.unwrap();

	// Act
	UserEvent::type_text(&username, " alice ");
	UserEvent::type_text(&email, "Alice@Example.COM");
	UserEvent::type_text(&password, "  secret password  ");
	let form: HtmlFormElement = username
		.closest("form")
		.unwrap()
		.unwrap()
		.dyn_into()
		.unwrap();
	form.request_submit().expect("registration submit");
	let screen_for_wait = screen.clone();
	wait_for(move || {
		screen_for_wait
			.get_by_role_with_name("button", "Sign in")
			.query()
			.is_some()
	})
	.await
	.expect("registration navigates to login");

	// Assert
	worker.calls_to_server_fn::<reinhardt_cloud_dashboard::apps::auth::server_fn::register::register::marker>().assert_called();
}

#[rstest::rstest]
#[test_attr(wasm_bindgen_test)]
async fn cluster_update_normalizes_bound_values_before_client_validation() {
	// Arrange
	let _env = wasm_test_env();
	let worker = msw_worker().await;
	worker.handle_server_fn::<me::marker>(|_| {
		Ok(UserInfo {
			id: "550e8400-e29b-41d4-a716-446655440000".to_owned(),
			username: "alice".to_owned(),
			email: "alice@example.com".to_owned(),
		})
	});
	worker
		.handle_server_fn::<list_clusters_for_current_org::marker>(|_| Ok(vec![cluster_fixture()]));
	worker.handle_server_fn::<reinhardt_cloud_dashboard::apps::clusters::server_fn::update_cluster_for_current_org::marker>(|args| {
		assert_eq!(args.request.cluster_id, "42");
		assert_eq!(args.request.name, "a".repeat(63));
		assert_eq!(args.request.api_url, "https://updated.example.com:6443");
		Ok(ClusterInfo { name: args.request.name, api_url: args.request.api_url, ..cluster_fixture() })
	});
	launch_dashboard_at("/clusters");
	let document = web_sys::window().unwrap().document().unwrap();
	let document_for_wait = document.clone();
	wait_for(move || {
		document_for_wait
			.query_selector("select option[value='42']")
			.unwrap()
			.is_some()
	})
	.await
	.expect("cluster selection loaded");
	let select: web_sys::HtmlSelectElement = document
		.query_selector("select")
		.unwrap()
		.unwrap()
		.dyn_into()
		.unwrap();
	select.set_value("42");
	select
		.dispatch_event(&web_sys::Event::new("change").unwrap())
		.unwrap();
	let name: HtmlInputElement = document
		.get_element_by_id("update-cluster-name")
		.unwrap()
		.dyn_into()
		.unwrap();
	// Act
	UserEvent::type_text(&name, &format!(" {} ", "a".repeat(63)));
	let api_url: HtmlInputElement = document
		.get_element_by_id("update-cluster-api-url")
		.unwrap()
		.dyn_into()
		.unwrap();
	UserEvent::type_text(&api_url, " https://updated.example.com:6443 ");
	let name: HtmlInputElement = document
		.get_element_by_id("update-cluster-name")
		.unwrap()
		.dyn_into()
		.unwrap();
	let form: HtmlFormElement = name.closest("form").unwrap().unwrap().dyn_into().unwrap();
	form.request_submit().expect("cluster update submit");
	let screen = screen();
	wait_for(move || screen.get_by_text("Cluster updated.").query().is_some())
		.await
		.expect("normalized cluster update succeeded");

	// Assert
	worker.calls_to_server_fn::<reinhardt_cloud_dashboard::apps::clusters::server_fn::update_cluster_for_current_org::marker>().assert_called();
}
