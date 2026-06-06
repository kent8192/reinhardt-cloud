//! Toast notification component.
//!
//! Container rendered via `page!` macro. Individual toasts added/removed
//! dynamically via web-sys DOM operations + gloo-timers auto-dismiss.

#[cfg(wasm)]
use reinhardt::pages::component::Page;
#[cfg(wasm)]
use reinhardt::pages::page;

use crate::shared::ws_messages::NotificationLevel;

/// Render the toast container overlay (empty; toasts added dynamically).
#[cfg(wasm)]
pub fn toast_container() -> Page {
	page!(|| {
		div {
			id: "toast-container",
			class: "fixed top-4 right-4 z-50 flex flex-col gap-2 max-w-sm",
		}
	})()
}

/// Dynamically add a toast notification to the container.
#[cfg(wasm)]
pub fn show_toast(level: &NotificationLevel, title: &str, message: &str) {
	let Some(window) = web_sys::window() else {
		return;
	};
	let Some(document) = window.document() else {
		return;
	};
	let Some(container) = document.get_element_by_id("toast-container") else {
		return;
	};

	let toast = document.create_element("div").unwrap();
	let (bg, border, icon) = toast_style(level);

	toast
		.set_attribute(
			"class",
			&format!("{bg} {border} border rounded-md shadow-lg p-4"),
		)
		.unwrap();

	let title_escaped = html_escape(title);
	let message_escaped = html_escape(message);

	toast.set_inner_html(&format!(
		r#"<div class="flex items-start gap-3"><span class="text-lg shrink-0">{icon}</span><div class="min-w-0"><p class="font-semibold text-sm text-ink-950">{title_escaped}</p><p class="text-sm text-ink-600 mt-0.5">{message_escaped}</p></div></div>"#
	));

	container.append_child(&toast).unwrap();

	let toast_clone = toast.clone();
	gloo_timers::callback::Timeout::new(5_000, move || {
		toast_clone.remove();
	})
	.forget();
}

/// Map notification level to CSS classes and icon.
pub fn toast_style(level: &NotificationLevel) -> (&'static str, &'static str, &'static str) {
	match level {
		NotificationLevel::Info => ("bg-blue-50", "border-blue-200", "\u{2139}\u{FE0F}"),
		NotificationLevel::Warning => ("bg-amber-50", "border-amber-200", "\u{26A0}\u{FE0F}"),
		NotificationLevel::Critical => ("bg-red-50", "border-red-200", "\u{274C}"),
	}
}

/// Minimal HTML escaping for text content.
pub fn html_escape(s: &str) -> String {
	s.replace('&', "&amp;")
		.replace('<', "&lt;")
		.replace('>', "&gt;")
		.replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case(NotificationLevel::Info, "bg-blue-50", "border-blue-200")]
	#[case(NotificationLevel::Warning, "bg-amber-50", "border-amber-200")]
	#[case(NotificationLevel::Critical, "bg-red-50", "border-red-200")]
	fn test_toast_style_returns_correct_classes(
		#[case] level: NotificationLevel,
		#[case] expected_bg: &str,
		#[case] expected_border: &str,
	) {
		// Act
		let (bg, border, _icon) = toast_style(&level);

		// Assert
		assert_eq!(bg, expected_bg);
		assert_eq!(border, expected_border);
	}

	#[rstest]
	#[case("hello", "hello")]
	#[case("<script>", "&lt;script&gt;")]
	#[case("a&b", "a&amp;b")]
	#[case(r#"he said "hi""#, "he said &quot;hi&quot;")]
	fn test_html_escape(#[case] input: &str, #[case] expected: &str) {
		// Act
		let result = html_escape(input);

		// Assert
		assert_eq!(result, expected);
	}
}
