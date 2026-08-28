//! Toast notification component.
//!
//! Container rendered via `page!` macro. Individual toasts added/removed
//! dynamically via web-sys DOM operations + gloo-timers auto-dismiss.

#[cfg(wasm)]
use reinhardt::pages::component::Page;
#[cfg(wasm)]
use reinhardt::pages::page;

use crate::shared::client::style::STYLES;
use crate::shared::ws_messages::NotificationLevel;

/// Render the toast container overlay (empty; toasts added dynamically).
#[cfg(wasm)]
pub fn toast_container() -> Page {
	page!({
		div {
			id: "toast-container",
			class: STYLES.toast_container(),
		}
	})
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
	let (variant, icon) = toast_style(level);

	toast
		.set_attribute("class", &format!("{} {}", STYLES.toast().as_str(), variant))
		.unwrap();

	let title_escaped = html_escape(title);
	let message_escaped = html_escape(message);

	toast.set_inner_html(&format!(
		r#"<div class="{}"><span class="{}">{icon}</span><div class="{}"><p class="{}">{title_escaped}</p><p class="{}">{message_escaped}</p></div></div>"#,
		STYLES.toast_content().as_str(),
		STYLES.toast_icon().as_str(),
		STYLES.toast_body().as_str(),
		STYLES.toast_title().as_str(),
		STYLES.toast_message().as_str(),
	));

	container.append_child(&toast).unwrap();

	let toast_clone = toast.clone();
	gloo_timers::callback::Timeout::new(5_000, move || {
		toast_clone.remove();
	})
	.forget();
}

/// Map notification level to CSS classes and icon.
pub fn toast_style(level: &NotificationLevel) -> (&'static str, &'static str) {
	match level {
		NotificationLevel::Info => (STYLES.toast_info().as_str(), "\u{2139}\u{FE0F}"),
		NotificationLevel::Warning => (STYLES.toast_warning().as_str(), "\u{26A0}\u{FE0F}"),
		NotificationLevel::Critical => (STYLES.toast_critical().as_str(), "\u{274C}"),
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
	#[case(NotificationLevel::Info, STYLES.toast_info().as_str())]
	#[case(NotificationLevel::Warning, STYLES.toast_warning().as_str())]
	#[case(NotificationLevel::Critical, STYLES.toast_critical().as_str())]
	fn test_toast_style_returns_correct_classes(
		#[case] level: NotificationLevel,
		#[case] expected_variant: &str,
	) {
		// Act
		let (variant, _icon) = toast_style(&level);

		// Assert
		assert_eq!(variant, expected_variant);
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
