//! Cluster health panel component.
//!
//! Renders a `<div id="cluster-health">` container populated with one row
//! per (`cluster_name`, `agent_id`) pair from incoming
//! [`ClusterHealthPayload`] WebSocket messages. Each (cluster, agent) key
//! uses a stable DOM id so subsequent updates replace the existing row
//! rather than duplicating it.

#[cfg(wasm)]
use reinhardt::pages::component::Page;
#[cfg(wasm)]
use reinhardt::pages::page;

use crate::shared::ws_messages::ClusterHealthPayload;

#[cfg(wasm)]
use crate::client::components::toast::html_escape;

/// DOM id of the cluster health container.
#[cfg(wasm)]
const CONTAINER_ID: &str = "cluster-health";

/// Render the cluster health container (empty; rows added dynamically).
#[cfg(wasm)]
pub fn cluster_health_container() -> Page {
	page!(|| {
		div {
			id: "cluster-health",
			class: "cluster-health grid gap-2",
		}
	})()
}

/// Insert or replace a cluster health row for the given payload.
#[cfg(wasm)]
pub fn update(payload: ClusterHealthPayload) {
	let Some(document) = web_sys::window().and_then(|w| w.document()) else {
		return;
	};
	let Some(container) = document.get_element_by_id(CONTAINER_ID) else {
		return;
	};

	let row_id = row_id(&payload.cluster_name, &payload.agent_id);
	let existing = document.get_element_by_id(&row_id);

	let row = match existing {
		Some(el) => el,
		None => {
			let Ok(el) = document.create_element("div") else {
				return;
			};
			let _ = el.set_attribute("id", &row_id);
			let _ = container.append_child(&el);
			el
		}
	};

	let status_class = if payload.healthy {
		"bg-green-50 border-green-200"
	} else {
		"bg-red-50 border-red-200"
	};
	let _ = row.set_attribute(
		"class",
		&format!(
			"cluster-health-row border rounded p-2 text-sm flex items-center gap-3 {status_class}"
		),
	);

	let cluster = html_escape(&payload.cluster_name);
	let agent = html_escape(&payload.agent_id);
	let ts = html_escape(&payload.timestamp);
	let status = if payload.healthy {
		"healthy"
	} else {
		"unhealthy"
	};
	let cpu = format!("{:.1}", payload.cpu_usage_percent);
	let mem = format!("{:.1}", payload.memory_usage_percent);
	let pods = payload.pod_count;

	row.set_inner_html(&format!(
		r#"<strong>{cluster}</strong><span class="text-gray-500">agent={agent}</span><span>status={status}</span><span>cpu={cpu}%</span><span>mem={mem}%</span><span>pods={pods}</span><span class="text-gray-400 ml-auto">{ts}</span>"#
	));
}

/// Compute a stable DOM id for a (cluster, agent) pair.
pub fn row_id(cluster_name: &str, agent_id: &str) -> String {
	format!("cluster-health-{cluster_name}-{agent_id}")
}

// Non-WASM stub so server-side callers (and unit tests) can compile.
#[cfg(not(wasm))]
#[allow(dead_code)]
pub fn update(_payload: ClusterHealthPayload) {}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_row_id_is_stable_and_composed() {
		// Arrange / Act
		let id = row_id("prod-east", "agent-17");

		// Assert
		assert_eq!(id, "cluster-health-prod-east-agent-17");
	}

	#[rstest]
	fn test_row_id_differs_by_agent() {
		// Act
		let a = row_id("c1", "agent-a");
		let b = row_id("c1", "agent-b");

		// Assert
		assert_ne!(a, b);
	}
}
