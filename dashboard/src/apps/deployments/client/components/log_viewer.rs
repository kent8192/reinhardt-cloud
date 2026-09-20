//! Real-time log viewer component.
//!
//! The viewer renders a `<pre id="log-viewer">` container. Incoming
//! `AppLog` and `BuildLog` WebSocket messages append `<span>` children with the
//! generated log-line token. The DOM buffer is capped at [`MAX_LINES`] entries to bound
//! memory — older lines are removed from the front when the cap is reached.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{QueryHandle, QueryOptions, QueryStatus, Signal, use_query};

use crate::apps::deployments::client::style::{STYLES, log_line_class};
use crate::apps::deployments::server_fn::{DeploymentLogInfo, deployment_logs_for_current_org};
use crate::shared::client::components::toast::html_escape;
use crate::shared::ws_messages::{AppLogPayload, BuildLogPayload};

/// Maximum number of log lines retained in the DOM buffer.
const MAX_LINES: usize = 1000;

/// DOM id of the log viewer container.
const CONTAINER_ID: &str = "log-viewer";

/// Render the log viewer container with historical lines for the selected deployment.
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

fn log_viewer_empty() -> Page {
	page!({
		pre {
			id: "log-viewer",
			class: STYLES.log_viewer(),
			span {
				class: STYLES.log_line() + STYLES.log_line_muted(),
				"Select a deployment to load logs."
			}
		}
	})
}

fn render_log_history(
	history: &QueryHandle<Vec<DeploymentLogInfo>, reinhardt::pages::server_fn::ServerFnError>,
) -> Page {
	let snapshot = history.snapshot();
	let content = match snapshot.status {
		QueryStatus::Idle => page!({
			span {
				class: STYLES.log_line() + STYLES.log_line_muted(),
				"Log history is not available during server rendering."
			}
		}),
		QueryStatus::Pending => page!({
			span {
				class: STYLES.log_line() + STYLES.log_line_muted(),
				"Loading logs..."
			}
		}),
		QueryStatus::Error => {
			let message = snapshot
				.error
				.map(|error| error.user_message().to_owned())
				.unwrap_or_else(|| "Unable to load logs.".to_owned());
			page!({
				span {
					class: STYLES.log_line() + STYLES.log_line_error(),
					{ message }
				}
			})
		}
		QueryStatus::Success => {
			let lines = snapshot.data.unwrap_or_default();
			let history = if lines.is_empty() {
				page!({
					span {
						class: STYLES.log_line() + STYLES.log_line_muted(),
						"No log entries."
					}
				})
			} else {
				page!({
					{
						lines
							.iter()
							.map(self::render_history_line)
							.collect::<Vec<_>>()
					}
				})
			};
			let refetch_notice = if let Some(error) = snapshot.refetch_error {
				let message = error.user_message().to_owned();
				page!({
					span {
						class: STYLES.log_line() + STYLES.log_line_warning(),
						{ format!("Showing cached logs: {message}") }
					}
				})
			} else if snapshot.is_fetching {
				page!({
					span {
						class: STYLES.log_line() + STYLES.log_line_muted(),
						"Refreshing logs..."
					}
				})
			} else {
				Page::Empty
			};
			page!({
				{
					refetch_notice
				}
				{ history }
			})
		}
	};

	page!({
		pre {
			id: "log-viewer",
			class: STYLES.log_viewer(),
			{ content }
		}
	})
}

fn render_history_line(line: &DeploymentLogInfo) -> Page {
	let timestamp = line.timestamp.clone();
	let level = line.level.clone();
	let message = line.message.clone();
	page!({
		span {
			class: log_line_class(&line.level),
			{ format!("[{timestamp}] [{level}] {message}") }
		}
	})
}

/// Append an application log line to the viewer.
pub fn append(payload: AppLogPayload) {
	append_line(
		&payload.timestamp,
		&payload.source,
		&payload.level,
		&payload.message,
	);
}

/// Append a build log line to the viewer.
pub fn append_build(payload: BuildLogPayload) {
	append_line(
		&payload.timestamp,
		&payload.build_id,
		&payload.event_type,
		&payload.message,
	);
}

/// Render a single log line into the viewer, enforcing the line cap.
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
	let class = log_line_class(level);
	let _ = line.set_attribute("class", class.as_str());

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
