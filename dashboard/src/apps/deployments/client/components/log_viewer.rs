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
#[cfg(wasm)]
use reinhardt::pages::prelude::{QueryHandle, QueryOptions, QueryStatus, Signal, use_query};

#[cfg(wasm)]
use crate::apps::deployments::server_fn::{DeploymentLogInfo, deployment_logs_for_current_org};
#[cfg(wasm)]
use crate::shared::client::components::toast::html_escape;
use crate::shared::ws_messages::{AppLogPayload, BuildLogPayload};

/// Maximum number of log lines retained in the DOM buffer.
#[cfg(any(wasm, test))]
const MAX_LINES: usize = 1000;

/// DOM id of the log viewer container.
#[cfg(wasm)]
const CONTAINER_ID: &str = "log-viewer";

/// Render the log viewer container with historical lines for the selected deployment.
#[cfg(wasm)]
pub fn log_viewer_container(deployment_id: Signal<String>) -> Page {
	Page::reactive(move || {
		let deployment_id = deployment_id.get();
		if deployment_id.trim().is_empty() {
			return log_viewer_empty();
		}

		let history = use_query(
			deployment_logs_for_current_org::query(deployment_id),
			QueryOptions::new(),
		);
		Page::reactive(move || render_log_history(&history))
	})
}

#[cfg(wasm)]
fn log_viewer_empty() -> Page {
	page!(|| {
		pre {
			id: "log-viewer",
			class: "log-viewer max-h-96 overflow-auto rounded-md bg-ink-950 p-3 font-mono text-xs text-gray-100 whitespace-pre-wrap",
			span {
				class: "block text-gray-400",
				"Select a deployment to load logs."
			}
		}
	})()
}

#[cfg(wasm)]
fn render_log_history(
	history: &QueryHandle<Vec<DeploymentLogInfo>, reinhardt::pages::server_fn::ServerFnError>,
) -> Page {
	let snapshot = history.snapshot();
	let content = match snapshot.status {
		QueryStatus::Idle => page!(|| {
			span {
				class: "block text-gray-400",
				"Log history is not available during server rendering."
			}
		})(),
		QueryStatus::Pending => page!(|| {
			span {
				class: "block text-gray-400",
				"Loading logs..."
			}
		})(),
		QueryStatus::Error => page!(|message: String| {
			span {
				class: "block text-red-300",
				{ message }
			}
		})(
			snapshot
				.error
				.map(|error| error.user_message().to_owned())
				.unwrap_or_else(|| "Unable to load logs.".to_owned()),
		),
		QueryStatus::Success => {
			let lines = snapshot.data.unwrap_or_default();
			let history = if lines.is_empty() {
				page!(|| {
					span {
						class: "block text-gray-400",
						"No log entries."
					}
				})()
			} else {
				page!(|lines: Vec<DeploymentLogInfo>| {
					{
						lines
							.iter()
							.map(self::render_history_line)
							.collect::<Vec<_>>()
					}
				})(lines)
			};
			let refetch_notice = if let Some(error) = snapshot.refetch_error {
				page!(|message: String| {
					span {
						class: "block text-amber-300",
						{ format!("Showing cached logs: {message}") }
					}
				})(error.user_message().to_owned())
			} else if snapshot.is_fetching {
				page!(|| {
					span {
						class: "block text-gray-400",
						"Refreshing logs..."
					}
				})()
			} else {
				Page::Empty
			};
			page!(|refetch_notice: Page, history: Page| {
				{
					refetch_notice
				}
				{ history }
			})(refetch_notice, history)
		}
	};

	page!(|content: Page| {
		pre {
			id: "log-viewer",
			class: "log-viewer max-h-96 overflow-auto rounded-md bg-ink-950 p-3 font-mono text-xs text-gray-100 whitespace-pre-wrap",
			{ content }
		}
	})(content)
}

#[cfg(not(wasm))]
pub fn log_viewer_container(
	_deployment_id: reinhardt::pages::prelude::Signal<String>,
) -> reinhardt::pages::component::Page {
	reinhardt::pages::component::Page::Empty
}

#[cfg(wasm)]
fn render_history_line(line: &DeploymentLogInfo) -> Page {
	let level_class = level_class(&line.level);
	page!(|timestamp: String, level: String, message: String, level_class: &'static str| {
		span {
			class: format!("log-line {level_class} block"),
			{ format!("[{timestamp}] [{level}] {message}") }
		}
	})(
		line.timestamp.clone(),
		line.level.clone(),
		line.message.clone(),
		level_class,
	)
}

/// Append an application log line to the viewer.
#[cfg(wasm)]
pub fn append(payload: AppLogPayload) {
	append_line(
		&payload.timestamp,
		&payload.source,
		&payload.level,
		&payload.message,
	);
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
	// `first_element_child()` returns `Element`, but `remove_child` expects
	// a `Node`. The `Deref` impl on `Element` does not expose `AsRef<Node>`,
	// so we use `Into::<web_sys::Node>::into` to convert explicitly.
	while container.child_element_count() as usize > MAX_LINES
		&& let Some(first) = container.first_element_child()
	{
		let node: web_sys::Node = first.into();
		let _ = container.remove_child(&node);
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
