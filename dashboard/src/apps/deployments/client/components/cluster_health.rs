//! Cluster health panel component.
//!
//! Renders a `<div id="cluster-health">` container populated with one row
//! per (`cluster_name`, `agent_id`) pair from incoming
//! `ClusterHealthPayload` WebSocket messages. Each (cluster, agent) key
//! uses a stable DOM id so subsequent updates replace the existing row
//! rather than duplicating it.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

use crate::apps::deployments::client::style::STYLES;
#[cfg(wasm)]
use crate::shared::client::components::toast::html_escape;
#[cfg(wasm)]
use crate::shared::ws_messages::ClusterHealthPayload;

/// DOM id of the cluster health container.
#[cfg(wasm)]
const CONTAINER_ID: &str = "cluster-health";

/// Render the cluster health container (empty; rows added dynamically).
pub fn cluster_health_container() -> Page {
	page!({
		div {
			id: "cluster-health",
			class: STYLES.cluster_health(),
		}
	})
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

	let row_class = cluster_health_row_class(payload.healthy);
	let _ = row.set_attribute("class", row_class.as_str());

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

	row.set_inner_html(&cluster_health_row_markup(
		&cluster, &agent, &ts, status, &cpu, &mem, pods,
	));
}

#[cfg(any(wasm, test))]
fn cluster_health_row_class(healthy: bool) -> reinhardt::pages::style::ClassList {
	if healthy {
		STYLES.cluster_health_row() + STYLES.cluster_health_healthy()
	} else {
		STYLES.cluster_health_row() + STYLES.cluster_health_unhealthy()
	}
}

#[cfg(any(wasm, test))]
fn cluster_health_row_markup(
	cluster: &str,
	agent: &str,
	timestamp: &str,
	status: &str,
	cpu: &str,
	memory: &str,
	pods: u32,
) -> String {
	format!(
		r#"<strong class="{}">{cluster}</strong><span class="{}">agent={agent}</span><span>status={status}</span><span>cpu={cpu}%</span><span>mem={memory}%</span><span>pods={pods}</span><span class="{}">{timestamp}</span>"#,
		STYLES.health_name().as_str(),
		STYLES.health_muted().as_str(),
		STYLES.health_timestamp().as_str(),
	)
}

/// Compute a stable DOM id for a (cluster, agent) pair.
///
/// Spaces and slashes in `cluster_name` or `agent_id` are replaced with `-`
/// to ensure the result is a valid HTML id token (spaces are not allowed and
/// slashes are not valid in unquoted CSS id selectors).
pub fn row_id(cluster_name: &str, agent_id: &str) -> String {
	let cluster = cluster_name.replace([' ', '/'], "-");
	let agent = agent_id.replace([' ', '/'], "-");
	format!("cluster-health-{cluster}-{agent}")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::apps::deployments::client::style::STYLES;
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

	#[rstest]
	fn test_row_id_normalizes_spaces() {
		// Spaces in cluster names or agent IDs must be replaced with `-` to
		// produce a valid HTML id token.
		let id = row_id("prod east", "agent 1");

		assert_eq!(id, "cluster-health-prod-east-agent-1");
	}

	#[rstest]
	fn test_row_id_normalizes_slashes() {
		// Slashes are not valid in unquoted CSS id selectors.
		let id = row_id("ns/cluster", "region/agent");

		assert_eq!(id, "cluster-health-ns-cluster-region-agent");
	}

	#[rstest]
	fn cluster_health_rows_use_generated_state_and_markup_tokens() {
		// Act
		let render: fn(&str, &str, &str, &str, &str, &str, u32) -> String =
			cluster_health_row_markup;
		let healthy = cluster_health_row_class(true);
		let unhealthy = cluster_health_row_class(false);
		let markup = render(
			"prod",
			"agent-a",
			"2026-08-28T00:00:00Z",
			"healthy",
			"1.0",
			"2.0",
			3,
		);

		// Assert
		assert_eq!(
			healthy.as_str(),
			(STYLES.cluster_health_row() + STYLES.cluster_health_healthy()).as_str()
		);
		assert_eq!(
			unhealthy.as_str(),
			(STYLES.cluster_health_row() + STYLES.cluster_health_unhealthy()).as_str()
		);
		assert_eq!(
			markup,
			format!(
				"<strong class=\"{}\">prod</strong><span class=\"{}\">agent=agent-a</span><span>status=healthy</span><span>cpu=1.0%</span><span>mem=2.0%</span><span>pods=3</span><span class=\"{}\">2026-08-28T00:00:00Z</span>",
				STYLES.health_name().as_str(),
				STYLES.health_muted().as_str(),
				STYLES.health_timestamp().as_str(),
			)
		);
	}
}
