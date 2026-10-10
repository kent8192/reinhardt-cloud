//! Browser tests for the signed-out layout and its theme toggle.
//!
//! The file is gated on the `client` cfg alias, so it compiles to nothing on
//! native targets and only runs under `cargo make wasm-test`.

#![cfg(client)]

use cloud_control_plane::i18n::i18n_context;
use cloud_control_plane::ui::button::{ButtonProps, button};
use cloud_control_plane::ui::dialog::{self, DialogProps, dialog};
use cloud_control_plane::ui::layout::signed_out::signed_out_layout;
use cloud_control_plane::ui::theme::{STORAGE_KEY, Theme, current_theme};
use reinhardt::pages::component::{Page, PageExt};
use reinhardt::pages::i18n::{I18nContext, provide_i18n_context};
use reinhardt::pages::reactive::ReactiveScope;
use reinhardt::pages::{Element as DomElement, document, page, t};
use rstest::{fixture, rstest};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use web_sys::HtmlElement;

wasm_bindgen_test_configure!(run_in_browser);

/// Mounts a page under a fresh test root and restores the document on drop.
///
/// Owning the reactive scope and the i18n guard here keeps every signal and
/// translation alive while the test inspects the DOM, and releases them (plus
/// the theme attribute and stored choice) even when an assertion panics.
struct Sandbox {
	root: DomElement,
	_i18n: reinhardt::pages::reactive::ContextGuard<I18nContext>,
	_scope: ReactiveScope,
}

impl Sandbox {
	fn new() -> Self {
		reset_theme();
		let doc = document();
		let root = doc.create_element("div").expect("create test root");
		root.set_attribute("id", "ui-test-root")
			.expect("set test root id");
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

	fn query(&self, selector: &str) -> web_sys::Element {
		self.root
			.as_web_sys()
			.query_selector(selector)
			.expect("valid selector")
			.unwrap_or_else(|| {
				panic!(
					"`{selector}` not found in {}",
					self.root.as_web_sys().inner_html()
				)
			})
	}
}

impl Drop for Sandbox {
	fn drop(&mut self) {
		self.root.as_web_sys().remove();
		reset_theme();
	}
}

fn reset_theme() {
	let root = web_sys::window()
		.and_then(|window| window.document())
		.and_then(|document| document.document_element())
		.expect("document element");
	root.remove_attribute("data-theme")
		.expect("clear theme attribute");
	web_sys::window()
		.and_then(|window| window.local_storage().ok().flatten())
		.expect("local storage")
		.remove_item(STORAGE_KEY)
		.expect("clear stored theme");
}

fn stored_theme() -> Option<String> {
	web_sys::window()
		.and_then(|window| window.local_storage().ok().flatten())
		.expect("local storage")
		.get_item(STORAGE_KEY)
		.expect("read stored theme")
}

fn document_theme() -> Option<String> {
	web_sys::window()
		.and_then(|window| window.document())
		.and_then(|document| document.document_element())
		.expect("document element")
		.get_attribute("data-theme")
}

/// Yields to the event loop once so reactive updates reach the DOM.
async fn next_tick() {
	let promise = js_sys::Promise::new(&mut |resolve, _reject| {
		web_sys::window()
			.expect("window")
			.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 0)
			.expect("schedule timeout");
	});
	wasm_bindgen_futures::JsFuture::from(promise)
		.await
		.expect("timeout promise resolves");
}

#[fixture]
fn sandbox() -> Sandbox {
	Sandbox::new()
}

#[rstest]
#[wasm_bindgen_test]
fn signed_out_layout_renders_its_landmarks(sandbox: Sandbox) {
	// Arrange
	let content = page!({
		h1 { "Welcome" }
	});

	// Act
	sandbox.mount(|| signed_out_layout(content, None));

	// Assert
	assert_eq!(
		sandbox.query("main#main h1").text_content().as_deref(),
		Some("Welcome")
	);
	assert_eq!(
		sandbox.query("a[href=\"#main\"]").text_content().as_deref(),
		Some("Skip to main content")
	);
	assert_eq!(
		sandbox
			.query("header p")
			.text_content()
			.map(|text| text.trim().to_owned()),
		Some("Reinhardt Cloud".to_owned())
	);
	assert_eq!(
		sandbox.query("header img").get_attribute("alt").as_deref(),
		Some("")
	);
}

#[rstest]
#[wasm_bindgen_test]
fn theme_toggle_switches_data_theme_and_remembers_the_choice(sandbox: Sandbox) {
	// Arrange
	sandbox.mount(|| {
		signed_out_layout(
			page!({
				h1 { "Welcome" }
			}),
			None,
		)
	});
	let initial = current_theme();
	let toggle = sandbox
		.query("header button")
		.dyn_into::<HtmlElement>()
		.expect("toggle is an HTML element");

	// Act
	toggle.click();
	let after_first = document_theme();
	let stored_after_first = stored_theme();
	toggle.click();

	// Assert
	assert_eq!(after_first.as_deref(), Some(initial.toggled().as_str()));
	assert_eq!(
		stored_after_first.as_deref(),
		Some(initial.toggled().as_str())
	);
	assert_eq!(document_theme().as_deref(), Some(initial.as_str()));
	assert_eq!(stored_theme().as_deref(), Some(initial.as_str()));
}

#[rstest]
#[test_attr(wasm_bindgen_test)]
async fn theme_toggle_label_names_the_theme_it_switches_to(sandbox: Sandbox) {
	// Arrange
	sandbox.mount(|| {
		signed_out_layout(
			page!({
				h1 { "Welcome" }
			}),
			None,
		)
	});
	let toggle = sandbox
		.query("header button")
		.dyn_into::<HtmlElement>()
		.expect("toggle is an HTML element");
	let initial = current_theme();
	let label = |theme: Theme| match theme {
		Theme::Dark => "Switch to light theme",
		Theme::Light => "Switch to dark theme",
	};

	// Act
	let before = toggle.text_content();
	toggle.click();
	next_tick().await;
	let after = toggle.text_content();

	// Assert
	assert_eq!(before.as_deref().map(str::trim), Some(label(initial)));
	assert_eq!(
		after.as_deref().map(str::trim),
		Some(label(initial.toggled()))
	);
}

#[rstest]
#[wasm_bindgen_test]
fn dialog_opens_as_a_modal_and_closes(sandbox: Sandbox) {
	// Arrange
	sandbox.mount(|| {
		dialog(DialogProps::new(
			"confirm",
			t!("Copy"),
			Page::text("Body text"),
			button(ButtonProps::new(t!("Copied"))),
		))
	});
	let element = sandbox.query("dialog#confirm");

	// Act
	let before = element.has_attribute("open");
	dialog::show_modal("confirm");
	let while_open = element.has_attribute("open");
	dialog::close("confirm");
	let after = element.has_attribute("open");

	// Assert
	assert!(!before);
	assert!(while_open);
	assert!(!after);
}
