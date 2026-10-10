//! Browser tests for the signed-out layout, theme toggle, portal dialog, copy
//! button, and the `Table` primitive with the design styles.
//!
//! The file is gated on the `client` cfg alias, so it compiles to nothing on
//! native targets and only runs under `cargo make wasm-test`.

#![cfg(client)]

use cloud_control_plane::components::button::{ButtonProps, button};
use cloud_control_plane::components::code_block::code_block;
use cloud_control_plane::components::dialog::{DialogProps, open_dialog};
use cloud_control_plane::components::layout::signed_out::signed_out_layout;
use cloud_control_plane::components::table_styles::TABLE_STYLES;
use cloud_control_plane::components::theme::{STORAGE_KEY, Theme, current_theme};
use std::cell::Cell;
use std::rc::Rc;

use cloud_control_plane::i18n::i18n_context;
use reinhardt::pages::builder::html::{div, table, tbody, tr};
use reinhardt::pages::component::{Page, PageExt};
use reinhardt::pages::i18n::{I18nContext, provide_i18n_context};
use reinhardt::pages::reactive::ReactiveScope;
use reinhardt::pages::tables::columns::Column;
use reinhardt::pages::tables::{ColumnTrait, SortDirection, Table};
use reinhardt::pages::{Element as DomElement, document, page, t};
use rstest::{fixture, rstest};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};
use web_sys::{HtmlDialogElement, HtmlElement};

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

/// Replaces `navigator.clipboard` and counts uncaught errors until dropped.
struct ClipboardStub;

impl ClipboardStub {
	fn run(script: &str) -> JsValue {
		js_sys::Function::new_no_args(script)
			.call0(&JsValue::NULL)
			.expect("test script runs")
	}

	fn install(clipboard: &str) -> Self {
		Self::run(&format!(
			"window.__rcUncaught = 0;
			window.__rcOnError = () => {{ window.__rcUncaught += 1; }};
			window.addEventListener('error', window.__rcOnError);
			Object.defineProperty(navigator, 'clipboard', {{ value: {clipboard}, configurable: true }});"
		));
		Self
	}

	/// A non-secure origin, where `navigator.clipboard` is `undefined`.
	fn missing() -> Self {
		Self::install("undefined")
	}

	/// A clipboard whose `writeText` resolves and records the copied text.
	fn resolving() -> Self {
		Self::install(
			"{ writeText: (text) => { window.__rcCopied = text; return Promise.resolve(); } }",
		)
	}

	fn copied_text(&self) -> Option<String> {
		Self::run("return window.__rcCopied;").as_string()
	}

	fn uncaught_errors(&self) -> f64 {
		Self::run("return window.__rcUncaught;")
			.as_f64()
			.expect("error counter is a number")
	}
}

impl Drop for ClipboardStub {
	fn drop(&mut self) {
		Self::run(
			"delete navigator.clipboard;
			delete window.__rcCopied;
			window.removeEventListener('error', window.__rcOnError);",
		);
	}
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
fn dialog_opens_as_a_modal_in_a_portal_and_closes_when_dropped(sandbox: Sandbox) {
	// Arrange
	let props = DialogProps::new(
		"confirm",
		t!("Copy"),
		Page::text("Body text"),
		button(ButtonProps::new(t!("Copied"))),
	);
	let find = || {
		document()
			.query_selector("dialog#confirm")
			.expect("valid selector")
	};

	// Act
	let open = sandbox
		._scope
		.enter(|| open_dialog(props, || {}))
		.expect("dialog mounts");
	let element = find().expect("dialog is mounted under the body");
	let modal = element.as_web_sys().has_attribute("open");
	let in_sandbox = sandbox
		.root
		.as_web_sys()
		.contains(Some(element.as_web_sys()));
	drop(open);
	let after_drop = find();

	// Assert
	assert!(modal);
	assert!(!in_sandbox);
	assert!(after_drop.is_none());
}

/// A table built on the `Table` primitive with the design's scoped classes.
struct MembersTable {
	name: Column<String>,
	rows: Vec<String>,
}

impl Table for MembersTable {
	type Row = String;

	fn rows(&self) -> Vec<&String> {
		self.rows.iter().collect()
	}

	fn columns(&self) -> Vec<&dyn ColumnTrait> {
		vec![&self.name]
	}

	fn render(&self) -> DomElement {
		let mut body = tbody();
		for row in &self.rows {
			body = body.child(tr().child(self.name.render(row)).build());
		}
		div()
			.class(TABLE_STYLES.wrap().as_str())
			.child(
				table()
					.class(TABLE_STYLES.table().as_str())
					.child(body.build())
					.build(),
			)
			.build()
	}

	fn handle_sort(&mut self, _field: &str, _direction: SortDirection) {}

	fn handle_pagination(&mut self, _page: usize) {}
}

#[rstest]
#[wasm_bindgen_test]
fn table_primitive_renders_with_the_design_classes(sandbox: Sandbox) {
	// Arrange
	let table = MembersTable {
		name: Column::new("name", "Name"),
		rows: vec!["alice".to_owned(), "bob".to_owned()],
	};

	// Act
	let rendered = table.render();
	sandbox
		.root
		.as_web_sys()
		.append_child(rendered.as_web_sys())
		.expect("append table");

	// Assert
	assert_eq!(
		sandbox.root.as_web_sys().inner_html(),
		format!(
			"<div class=\"{}\"><table class=\"{}\"><tbody><tr><td>alice</td></tr><tr><td>bob</td></tr></tbody></table></div>",
			TABLE_STYLES.wrap().as_str(),
			TABLE_STYLES.table().as_str(),
		)
	);
}

#[rstest]
#[test_attr(wasm_bindgen_test)]
async fn copy_button_does_nothing_when_the_clipboard_is_unavailable(sandbox: Sandbox) {
	// Arrange
	let no_clipboard = ClipboardStub::missing();
	sandbox.mount(|| {
		code_block(
			"login",
			t!("Skip to main content"),
			"reinhardt-cloud login".to_owned(),
		)
	});
	let copy = sandbox
		.query("button[aria-controls=\"login-text\"]")
		.dyn_into::<HtmlElement>()
		.expect("copy button is an HTML element");

	// Act
	copy.click();
	next_tick().await;

	// Assert
	assert_eq!(no_clipboard.uncaught_errors(), 0.0);
	assert_eq!(
		copy.text_content().as_deref(),
		Some("Copy Skip to main content")
	);
}

#[rstest]
#[test_attr(wasm_bindgen_test)]
async fn copy_button_name_follows_the_copied_state_and_keeps_the_visible_word(sandbox: Sandbox) {
	// Arrange
	let clipboard = ClipboardStub::resolving();
	sandbox.mount(|| {
		code_block(
			"login",
			t!("Skip to main content"),
			"reinhardt-cloud login".to_owned(),
		)
	});
	let copy = sandbox
		.query("button[aria-controls=\"login-text\"]")
		.dyn_into::<HtmlElement>()
		.expect("copy button is an HTML element");
	let before = copy.text_content();

	// Act
	copy.click();
	next_tick().await;
	next_tick().await;
	let after = copy.text_content();

	// Assert
	assert_eq!(before.as_deref(), Some("Copy Skip to main content"));
	assert_eq!(after.as_deref(), Some("Copied Skip to main content"));
	assert_eq!(
		clipboard.copied_text().as_deref(),
		Some("reinhardt-cloud login")
	);
	assert_eq!(clipboard.uncaught_errors(), 0.0);
}

#[rstest]
#[test_attr(wasm_bindgen_test)]
async fn dialog_closed_by_the_browser_tears_down_the_portal_and_notifies(sandbox: Sandbox) {
	// Arrange
	let closed = Rc::new(Cell::new(0_u32));
	let props = DialogProps::new(
		"confirm",
		t!("Copy"),
		Page::text("Body text"),
		button(ButtonProps::new(t!("Copied"))),
	);
	let open = sandbox
		._scope
		.enter(|| {
			let closed = Rc::clone(&closed);
			open_dialog(props, move || closed.set(closed.get() + 1))
		})
		.expect("dialog mounts");
	let dialog = document()
		.query_selector("dialog#confirm")
		.expect("valid selector")
		.expect("dialog is mounted under the body")
		.as_web_sys()
		.clone()
		.dyn_into::<HtmlDialogElement>()
		.expect("dialog element");

	// Act
	dialog.close();
	next_tick().await;

	// Assert
	assert_eq!(closed.get(), 1);
	assert!(!open.is_open());
	assert!(
		document()
			.query_selector("dialog#confirm")
			.expect("valid selector")
			.is_none()
	);
	assert!(
		document()
			.query_selector("[data-rh-portal-host]")
			.expect("valid selector")
			.is_none()
	);
}
