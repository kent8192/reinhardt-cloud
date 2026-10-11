//! Browser test of moving between signed-in pages in the launched application.
//!
//! It launches the real client with the project routes, signed in, and follows
//! the links of the shell: the User menu's account link and the navigation's
//! Overview. A launched application installs page-wide state, so this lives in
//! its own test binary.

#![cfg(client)]

use cloud_control_plane::apps::accounts::serializers::viewer::Viewer;
use cloud_control_plane::apps::accounts::server_fn::current_viewer::current_viewer;
use cloud_control_plane::apps::accounts::urls::paths::{ACCOUNT_PAGE_PATH, HOME_PATH};
use cloud_control_plane::i18n::i18n_context;
use reinhardt::pages::ClientLauncher;
use reinhardt::pages::document;
use reinhardt::test::fixtures::wasm::msw::msw_worker;
use rstest::rstest;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

async fn settle() {
	for _ in 0..40 {
		let promise = js_sys::Promise::new(&mut |resolve, _reject| {
			web_sys::window()
				.expect("window")
				.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 5)
				.expect("schedule timeout");
		});
		wasm_bindgen_futures::JsFuture::from(promise)
			.await
			.expect("timeout promise resolves");
	}
}

fn query(root: &web_sys::Element, selector: &str) -> web_sys::Element {
	root.query_selector(selector)
		.expect("valid selector")
		.unwrap_or_else(|| panic!("`{selector}` not found in {}", root.inner_html()))
}

fn click(element: web_sys::Element) {
	element
		.dyn_into::<web_sys::HtmlElement>()
		.expect("an HTML element")
		.click();
}

#[rstest]
#[test_attr(wasm_bindgen_test)]
async fn the_shell_links_move_between_the_signed_in_pages_and_mark_the_current_one() {
	// Arrange
	let worker = msw_worker().await;
	worker.handle_server_fn::<current_viewer::marker>(|_| {
		Ok(Some(Viewer {
			github_login: "riley-chen".to_owned(),
			display_name: "Riley Chen".to_owned(),
			is_staff: false,
		}))
	});
	let window = web_sys::window().expect("window");
	window
		.history()
		.expect("history")
		.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(HOME_PATH))
		.expect("start at home");
	let root = document().create_element("div").expect("create root");
	root.set_attribute("id", "launch-root").expect("set id");
	document()
		.body()
		.expect("body")
		.as_web_sys()
		.append_child(root.as_web_sys())
		.expect("append root");
	ClientLauncher::new("#launch-root")
		.i18n_context(i18n_context())
		.router_client(|| cloud_control_plane::config::urls::routes().into_client())
		.launch()
		.expect("the client launches");
	settle().await;
	let root = root.as_web_sys();
	assert_eq!(
		query(root, "main h1").text_content().as_deref(),
		Some("Overview")
	);
	assert_eq!(
		query(root, "nav a[aria-current]")
			.get_attribute("href")
			.as_deref(),
		Some(HOME_PATH)
	);

	// Act
	click(query(root, "details[data-menu] a"));
	settle().await;

	// Assert
	assert_eq!(
		window.location().pathname().expect("path"),
		ACCOUNT_PAGE_PATH
	);
	assert_eq!(
		query(root, "main h1").text_content().as_deref(),
		Some("Account")
	);
	assert!(
		root.query_selector("nav a[aria-current]")
			.expect("selector")
			.is_none(),
		"no navigation entry is current on the account page"
	);

	// Act again
	click(query(root, "nav a"));
	settle().await;

	// Assert again
	assert_eq!(window.location().pathname().expect("path"), HOME_PATH);
	assert_eq!(
		query(root, "main h1").text_content().as_deref(),
		Some("Overview")
	);
	assert_eq!(
		query(root, "nav a[aria-current]")
			.get_attribute("aria-current")
			.as_deref(),
		Some("page")
	);
}
