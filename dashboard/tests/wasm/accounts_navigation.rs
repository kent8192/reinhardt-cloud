//! Browser test of the launched application's route guard (SR-09).
//!
//! It launches the real client with the project routes so the guard navigates
//! through the real router. A launched application
//! installs page-wide state, so this lives in its own test binary.

#![cfg(client)]

use cloud_control_plane::apps::accounts::server_fn::current_viewer::current_viewer;
use cloud_control_plane::apps::accounts::server_fn::take_sign_in_notice::take_sign_in_notice;
use cloud_control_plane::apps::accounts::urls::paths::{HOME_PATH, SIGN_IN_PAGE_PATH};
use cloud_control_plane::i18n::i18n_context;
use reinhardt::pages::ClientLauncher;
use reinhardt::pages::document;
use reinhardt::test::fixtures::wasm::msw::msw_worker;
use rstest::rstest;
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

#[rstest]
#[test_attr(wasm_bindgen_test)]
async fn sr_09_an_anonymous_browser_is_sent_to_the_sign_in_page_instead_of_the_private_page() {
	// Arrange
	let worker = msw_worker().await;
	worker.handle_server_fn::<current_viewer::marker>(|_| Ok(None));
	worker.handle_server_fn::<take_sign_in_notice::marker>(|_| Ok(None));
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

	// Act
	ClientLauncher::new("#launch-root")
		.i18n_context(i18n_context())
		// The inventory collection is a start-function concern; the test hands
		// the launcher the project routes directly.
		.router_client(|| cloud_control_plane::config::urls::routes().into_client())
		.launch()
		.expect("the client launches");
	settle().await;

	// Assert
	assert_eq!(
		window.location().pathname().expect("path"),
		SIGN_IN_PAGE_PATH
	);
	let heading = root
		.as_web_sys()
		.query_selector("h1")
		.expect("selector")
		.and_then(|heading| heading.text_content());
	assert_eq!(
		heading.as_deref(),
		Some("Run your Reinhardt Projects on your own Clusters."),
		"the sign-in page is shown, not the private page"
	);
}
