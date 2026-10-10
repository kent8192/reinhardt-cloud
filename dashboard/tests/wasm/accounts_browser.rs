//! Browser tests of the accounts application's pages: the sign-in page (SR-01,
//! SR-19) and the signed-in landing page.
//!
//! The file is gated on the `client` cfg alias, so it compiles to nothing on
//! native targets and only runs under `cargo make wasm-test`. Server functions
//! are answered by the mock service worker; nothing reaches a server.

#![cfg(client)]

use cloud_control_plane::apps::accounts::client::components::home::home_content;
use cloud_control_plane::apps::accounts::client::components::sign_in::{
	notice_alert, sign_in_page,
};
use cloud_control_plane::apps::accounts::serializers::sign_in::SignInNotice;
use cloud_control_plane::apps::accounts::serializers::viewer::Viewer;
use cloud_control_plane::apps::accounts::server_fn::take_sign_in_notice::take_sign_in_notice;
use cloud_control_plane::apps::accounts::urls::paths::{
	AUTH_PREFIX, GITHUB_SIGN_IN_PATH, HOME_PATH, SIGN_IN_PAGE_PATH,
};
use cloud_control_plane::apps::accounts::urls::reverse;
use cloud_control_plane::i18n::i18n_context;
use reinhardt::pages::component::{Page, PageExt};
use reinhardt::pages::i18n::{I18nContext, provide_i18n_context};
use reinhardt::pages::reactive::ReactiveScope;
use reinhardt::pages::{Element as DomElement, document};
use reinhardt::test::fixtures::wasm::msw::msw_worker;
use reinhardt::test::msw::MockServiceWorker;
use rstest::rstest;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

/// Mounts a page under a fresh test root and removes it on drop.
struct Sandbox {
	root: DomElement,
	_i18n: reinhardt::pages::reactive::ContextGuard<I18nContext>,
	_scope: ReactiveScope,
}

impl Sandbox {
	fn new() -> Self {
		let doc = document();
		let root = doc.create_element("div").expect("create test root");
		doc.body()
			.expect("document body")
			.as_web_sys()
			.append_child(root.as_web_sys())
			.expect("append test root");
		Self {
			root,
			_i18n: provide_i18n_context(i18n_context()),
			_scope: ReactiveScope::new(),
		}
	}

	fn mount(&self, build: impl FnOnce() -> Page) {
		self._scope
			.enter(|| build().mount(&self.root))
			.expect("mount page");
	}

	fn find(&self, selector: &str) -> Option<web_sys::Element> {
		self.root
			.as_web_sys()
			.query_selector(selector)
			.expect("valid selector")
	}

	fn query(&self, selector: &str) -> web_sys::Element {
		self.find(selector).unwrap_or_else(|| {
			panic!(
				"`{selector}` not found in {}",
				self.root.as_web_sys().inner_html()
			)
		})
	}

	fn text(&self) -> String {
		self.root.as_web_sys().text_content().unwrap_or_default()
	}
}

impl Drop for Sandbox {
	fn drop(&mut self) {
		self.root.as_web_sys().remove();
	}
}

/// Yields to the event loop so resources and reactive updates settle.
async fn settle() {
	for _ in 0..20 {
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

async fn worker_answering(notice: Option<SignInNotice>) -> MockServiceWorker {
	let worker = msw_worker().await;
	worker.handle_server_fn::<take_sign_in_notice::marker>(move |_| Ok(notice.clone()));
	worker
}

#[rstest]
#[test_attr(wasm_bindgen_test)]
async fn the_sign_in_page_offers_github_as_the_only_way_in() {
	// Arrange
	let _worker = worker_answering(None).await;
	let sandbox = Sandbox::new();

	// Act
	sandbox.mount(sign_in_page);
	settle().await;

	// Assert
	assert_eq!(
		sandbox.query("main#main h1").text_content().as_deref(),
		Some("Run your Reinhardt Projects on your own Clusters.")
	);
	let github = sandbox.query("main#main a[rel='external']");
	assert_eq!(
		github.text_content().as_deref(),
		Some("Continue with GitHub")
	);
	assert_eq!(
		github.get_attribute("href").as_deref(),
		Some(GITHUB_SIGN_IN_PATH)
	);
	assert!(
		sandbox.find("main#main input, main#main form").is_none(),
		"there is nowhere to type a credential (SR-01)"
	);
	assert!(
		sandbox
			.text()
			.contains("There are no passwords to set or reset.")
	);
	assert!(
		sandbox
			.find("main#main [role='alert'], main#main [role='status']")
			.is_none(),
		"nothing to explain yet"
	);
}

#[rstest]
#[test_attr(wasm_bindgen_test)]
async fn the_decorative_preview_is_hidden_from_assistive_technology() {
	// Arrange
	let _worker = worker_answering(None).await;
	let sandbox = Sandbox::new();

	// Act
	sandbox.mount(sign_in_page);
	settle().await;

	// Assert
	let aside = sandbox.query("[aria-hidden='true']");
	assert!(aside.has_attribute("inert"));
	assert!(
		aside
			.text_content()
			.unwrap_or_default()
			.contains("storefront-api")
	);
	assert!(
		sandbox
			.query("main#main")
			.text_content()
			.unwrap_or_default()
			.find("storefront-api")
			.is_none(),
		"the preview is outside the main landmark"
	);
	assert_eq!(
		aside
			.query_selector_all("a, button, input")
			.expect("selector")
			.length(),
		0
	);
}

#[rstest]
#[test_attr(wasm_bindgen_test)]
async fn a_refused_account_reads_the_invitation_alert_with_its_own_login() {
	// Arrange
	let _worker = worker_answering(Some(SignInNotice::NotInvited {
		login: "riley-chen".to_owned(),
	}))
	.await;
	let sandbox = Sandbox::new();

	// Act
	sandbox.mount(sign_in_page);
	settle().await;

	// Assert
	// A warning is announced politely (`status`); only a failure is `alert`.
	let alert = sandbox.query("main#main [role='status']");
	let text = alert.text_content().unwrap_or_default();
	assert!(
		text.contains("No Invitation found for @riley-chen"),
		"{text}"
	);
	assert!(
		text.contains("accepts new Users by Invitation only"),
		"{text}"
	);
	assert_eq!(
		sandbox
			.query("main#main a[rel='external']")
			.text_content()
			.as_deref(),
		Some("Continue with GitHub")
	);
}

#[rstest]
#[test_attr(wasm_bindgen_test)]
async fn a_failed_sign_in_does_not_explain_itself() {
	// Arrange
	let _worker = worker_answering(Some(SignInNotice::Failed)).await;
	let sandbox = Sandbox::new();

	// Act
	sandbox.mount(sign_in_page);
	settle().await;

	// Assert
	let text = sandbox
		.query("[role='alert']")
		.text_content()
		.unwrap_or_default();
	assert!(text.contains("Sign-in did not complete"), "{text}");
	assert!(!text.contains("Invitation"), "{text}");
}

#[rstest]
#[wasm_bindgen_test]
fn the_invitation_alert_never_reveals_which_policy_refused_the_account() {
	// Arrange
	let sandbox = Sandbox::new();

	// Act
	sandbox.mount(|| {
		notice_alert(&SignInNotice::NotInvited {
			login: "octocat".to_owned(),
		})
	});

	// Assert
	let text = sandbox.text().to_ascii_lowercase();
	for revealed in ["allowlist", "allow-list", "organization id", "policy"] {
		assert!(!text.contains(revealed), "{text}");
	}
}

#[rstest]
#[wasm_bindgen_test]
fn the_signed_in_landing_shows_who_is_signed_in() {
	// Arrange
	let sandbox = Sandbox::new();
	let viewer = Viewer {
		github_login: "riley-chen".to_owned(),
		display_name: "Riley Chen".to_owned(),
		avatar_url: None,
		is_staff: false,
	};

	// Act
	sandbox.mount(|| home_content(&viewer, Page::text("sign out here")));

	// Assert
	assert_eq!(
		sandbox.query("h1").text_content().as_deref(),
		Some("Signed in as Riley Chen")
	);
	assert!(sandbox.text().contains("@riley-chen"));
	assert!(sandbox.text().contains("sign out here"));
}

#[rstest]
#[wasm_bindgen_test]
fn the_client_routes_and_the_server_redirects_agree() {
	// Arrange / Act / Assert
	assert_eq!(reverse("sign-in", &[]), SIGN_IN_PAGE_PATH);
	assert_eq!(reverse("home", &[]), HOME_PATH);
	assert_eq!(GITHUB_SIGN_IN_PATH, format!("{AUTH_PREFIX}github/"));
}
