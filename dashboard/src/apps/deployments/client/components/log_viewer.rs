//! Real-time log viewer component.
//!
//! The viewer renders a `<pre id="log-viewer">` container. Incoming
//! `AppLog` and `BuildLog` WebSocket messages append `<span class="log-line">`
//! children. The DOM buffer is capped at [`MAX_LINES`] entries to bound
//! memory — older lines are removed from the front when the cap is reached.

#[cfg(wasm)]
use reinhardt::pages::component::Page;
#[cfg(wasm)]
use reinhardt::pages::page;

use crate::shared::ws_messages::{AppLogPayload, BuildLogPayload};

#[cfg(wasm)]
use crate::client::components::toast::html_escape;

/// Maximum number of log lines retained in the DOM buffer.
const MAX_LINES: usize = 1000;

/// DOM id of the log viewer container.
const CONTAINER_ID: &str = "log-viewer";

/// Render the log viewer container (empty; lines appended dynamically).
#[cfg(wasm)]
pub fn log_viewer_container() -> Page {
	page!(|| {
		pre {
			id: "log-viewer",
			class: "log-viewer bg-gray-900 text-gray-100 text-xs font-mono p-3 rounded overflow-auto max-h-96 whitespace-pre-wrap",
		}
	})()
}

/// Append an application log line to the viewer.
#[cfg(wasm)]
pub fn append(payload: AppLogPayload) {
	append_line(&payload.timestamp, &payload.source, &payload.level, &payload.message);
}

/// Append a build log line to the viewer.
#[cfg(wasm)]
pub fn append_build(payload: BuildLogPayload) {
	append_line(
		&payload.timestamp,
		&payload.build_id,
		&payload.event_type,
		&payload.message,
	);
}

/// Render a single log line into the viewer, enforcing the line cap.
#[cfg(wasm)]
fn append_line(timestamp: &str, source: &str, level: &str, message: &str) {
	let Some(document) = web_sys::window().and_then(|w| w.document()) else {
		return;
	};
	let Some(container) = document.get_element_by_id(CONTAINER_ID) else {
		return;
	};

	let Ok(line) = document.create_element("span") else {
		return;
	};
	let level_class = level_class(level);
	let _ = line.set_attribute("class", &format!("log-line {level_class} block"));

	let ts = html_escape(timestamp);
	let src = html_escape(source);
	let lvl = html_escape(level);
	let msg = html_escape(message);
	line.set_inner_html(&format!("[{ts}] [{src}] [{lvl}] {msg}"));

	let _ = container.append_child(&line);

	// Enforce the line cap by removing oldest children.
	while container.child_element_count() as usize > MAX_LINES
		&& let Some(first) = container.first_element_child()
	{
		let _ = container.remove_child(&first);
	}
}

/// Map a lowercase log level string to a CSS color class.
pub fn level_class(level: &str) -> &'static str {
	match level {
		"error" => "text-red-400",
		"warn" => "text-amber-300",
		"debug" => "text-gray-400",
		_ => "text-gray-100",
	}
}

// Non-WASM stubs so server-side callers (and unit tests) can compile.
#[cfg(not(wasm))]
#[allow(dead_code)]
pub fn append(_payload: AppLogPayload) {}

#[cfg(not(wasm))]
#[allow(dead_code)]
pub fn append_build(_payload: BuildLogPayload) {}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case("error", "text-red-400")]
	#[case("warn", "text-amber-300")]
	#[case("debug", "text-gray-400")]
	#[case("info", "text-gray-100")]
	#[case("unknown", "text-gray-100")]
	fn test_level_class_maps_known_levels(#[case] level: &str, #[case] expected: &str) {
		// Act
		let class = level_class(level);

		// Assert
		assert_eq!(class, expected);
	}

	#[rstest]
	fn test_max_lines_is_1000() {
		// Guard against accidental regressions of the DOM buffer cap.
		assert_eq!(MAX_LINES, 1000);
	}
}
