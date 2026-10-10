//! Browser integration for design-system components.
//!
//! Each function does its work in the browser and is an inert stub on the
//! server target, so components stay cfg-clean and render identically in
//! native component tests. The server variants never invoke their callbacks.

#[cfg(client)]
mod browser_impl {
	use js_sys::Reflect;
	use wasm_bindgen::closure::Closure;
	use wasm_bindgen::{JsCast, JsValue};
	use web_sys::{HtmlDialogElement, Window};

	fn window() -> Option<Window> {
		web_sys::window()
	}

	pub(super) fn document_theme() -> Option<String> {
		window()?
			.document()?
			.document_element()?
			.get_attribute("data-theme")
	}

	pub(super) fn set_document_theme(theme: &str) {
		let root = window()
			.and_then(|window| window.document())
			.and_then(|document| document.document_element());
		if let Some(root) = root {
			// Setting an attribute with a plain value cannot fail.
			let _ = root.set_attribute("data-theme", theme);
		}
	}

	pub(super) fn location_fragment() -> Option<String> {
		let hash = window()?.location().hash().ok()?;
		hash.strip_prefix('#')
			.filter(|fragment| !fragment.is_empty())
			.map(str::to_owned)
	}

	pub(super) fn read_stored(key: &str) -> Option<String> {
		// Storage can be blocked by the browser; treat that as "nothing stored".
		window()?.local_storage().ok()??.get_item(key).ok()?
	}

	pub(super) fn write_stored(key: &str, value: &str) {
		if let Some(Ok(Some(storage))) = window().map(|window| window.local_storage()) {
			// A blocked or full storage only loses persistence, not the theme.
			let _ = storage.set_item(key, value);
		}
	}

	fn color_scheme_query() -> Option<web_sys::MediaQueryList> {
		window()?.match_media("(prefers-color-scheme: dark)").ok()?
	}

	pub(super) fn system_prefers_dark() -> bool {
		color_scheme_query().is_some_and(|query| query.matches())
	}

	/// Removes an event listener from its target when dropped.
	pub(super) struct EventListener {
		target: web_sys::EventTarget,
		event: &'static str,
		closure: Closure<dyn FnMut()>,
	}

	impl Drop for EventListener {
		fn drop(&mut self) {
			// Removing a listener that is already gone is harmless.
			let _ = self.target.remove_event_listener_with_callback(
				self.event,
				self.closure.as_ref().unchecked_ref(),
			);
		}
	}

	fn listen(
		target: web_sys::EventTarget,
		event: &'static str,
		callback: impl FnMut() + 'static,
	) -> Option<EventListener> {
		let closure = Closure::<dyn FnMut()>::new(callback);
		target
			.add_event_listener_with_callback(event, closure.as_ref().unchecked_ref())
			.ok()?;
		Some(EventListener {
			target,
			event,
			closure,
		})
	}

	pub(super) fn on_system_theme_change(
		callback: impl FnMut() + 'static,
	) -> Option<EventListener> {
		listen(color_scheme_query()?.into(), "change", callback)
	}

	fn dialog(id: &str) -> Option<HtmlDialogElement> {
		window()?
			.document()?
			.get_element_by_id(id)?
			.dyn_into::<HtmlDialogElement>()
			.ok()
	}

	pub(super) fn show_modal(id: &str) {
		if let Some(dialog) = dialog(id) {
			// `showModal` throws only when the dialog is already open or detached.
			let _ = dialog.show_modal();
		}
	}

	pub(super) fn on_dialog_close(
		id: &str,
		callback: impl FnMut() + 'static,
	) -> Option<EventListener> {
		listen(dialog(id)?.into(), "close", callback)
	}

	pub(super) fn copy_text(text: &str, done: impl FnOnce() + 'static) {
		let Some(window) = window() else {
			return;
		};
		let navigator = window.navigator();
		// `navigator.clipboard` is undefined on non-secure origins; calling
		// through it would throw, so skip the copy and leave the label as is.
		let clipboard = Reflect::get(&navigator, &JsValue::from_str("clipboard"));
		if clipboard.is_ok_and(|value| value.is_undefined() || value.is_null()) {
			return;
		}
		let promise = navigator.clipboard().write_text(text);
		wasm_bindgen_futures::spawn_local(async move {
			// A rejected write (permission denied) leaves the label unchanged.
			if wasm_bindgen_futures::JsFuture::from(promise).await.is_ok() {
				done();
			}
		});
	}

	pub(super) fn after_millis(milliseconds: i32, callback: impl FnOnce() + 'static) {
		let Some(window) = window() else {
			return;
		};
		let callback = Closure::once_into_js(callback);
		// The closure is consumed by the single invocation, so nothing leaks.
		let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
			callback.unchecked_ref(),
			milliseconds,
		);
	}
}

#[cfg(server)]
mod browser_impl {
	pub(super) fn document_theme() -> Option<String> {
		None
	}

	pub(super) fn set_document_theme(_theme: &str) {}

	pub(super) fn location_fragment() -> Option<String> {
		None
	}

	pub(super) fn read_stored(_key: &str) -> Option<String> {
		None
	}

	pub(super) fn write_stored(_key: &str, _value: &str) {}

	pub(super) fn system_prefers_dark() -> bool {
		false
	}

	pub(super) struct EventListener;

	pub(super) fn on_system_theme_change(
		_callback: impl FnMut() + 'static,
	) -> Option<EventListener> {
		None
	}

	pub(super) fn show_modal(_id: &str) {}

	pub(super) fn on_dialog_close(
		_id: &str,
		_callback: impl FnMut() + 'static,
	) -> Option<EventListener> {
		None
	}

	pub(super) fn copy_text(_text: &str, _done: impl FnOnce() + 'static) {}

	pub(super) fn after_millis(_milliseconds: i32, _callback: impl FnOnce() + 'static) {}
}

/// Returns the `data-theme` attribute of the document root, if set.
pub fn document_theme() -> Option<String> {
	browser_impl::document_theme()
}

/// Sets the `data-theme` attribute of the document root.
pub fn set_document_theme(theme: &str) {
	browser_impl::set_document_theme(theme);
}

/// The fragment of the page URL without its `#`, if there is a non-empty one.
///
/// The browser never sends a fragment to the server, which is why a Login Link
/// keeps its secret there. Always `None` on the server target.
pub fn location_fragment() -> Option<String> {
	browser_impl::location_fragment()
}

/// Reads a value from local storage; `None` when absent or storage is blocked.
pub fn read_stored(key: &str) -> Option<String> {
	browser_impl::read_stored(key)
}

/// Writes a value to local storage; silently skipped when storage is blocked.
pub fn write_stored(key: &str, value: &str) {
	browser_impl::write_stored(key, value);
}

/// Reports whether the system theme is dark.
pub fn system_prefers_dark() -> bool {
	browser_impl::system_prefers_dark()
}

/// Keeps a browser event listener registered until dropped.
pub struct EventListener {
	_inner: browser_impl::EventListener,
}

/// Calls `callback` whenever the system color scheme changes between light
/// and dark.
///
/// Returns `None` when the browser cannot evaluate the media query (always on
/// the server target). The listener is removed when the returned guard is
/// dropped.
pub fn on_system_theme_change(callback: impl FnMut() + 'static) -> Option<EventListener> {
	browser_impl::on_system_theme_change(callback).map(|inner| EventListener { _inner: inner })
}

/// Opens the `<dialog>` with the given ID as a modal.
pub fn show_modal(id: &str) {
	browser_impl::show_modal(id);
}

/// Calls `callback` whenever the `<dialog>` with the given ID closes, whether
/// through `Escape`, `<form method="dialog">`, or `close()`.
///
/// Returns `None` when the dialog is not in the document (always on the server
/// target). The listener is removed when the returned guard is dropped.
pub fn on_dialog_close(id: &str, callback: impl FnMut() + 'static) -> Option<EventListener> {
	browser_impl::on_dialog_close(id, callback).map(|inner| EventListener { _inner: inner })
}

/// Copies `text` to the clipboard and calls `done` once the copy succeeded.
pub fn copy_text(text: &str, done: impl FnOnce() + 'static) {
	browser_impl::copy_text(text, done);
}

/// Calls `callback` once after `milliseconds`.
pub fn after_millis(milliseconds: i32, callback: impl FnOnce() + 'static) {
	browser_impl::after_millis(milliseconds, callback);
}
