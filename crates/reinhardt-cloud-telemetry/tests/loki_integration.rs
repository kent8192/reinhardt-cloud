//! Integration test: push a line to a real Loki container, then read it via
//! `list_logs` and `tail_logs`. Requires Docker (TestContainers) and the
//! `integration` feature:
//!
//! ```bash
//! cargo nextest run -p reinhardt-cloud-telemetry --features integration \
//!   --test loki_integration
//! ```

#![cfg(feature = "integration")]

use std::time::Duration;

use reinhardt_cloud_core::pagination::PaginationParams;
use reinhardt_cloud_core::traits::LogService;
use reinhardt_cloud_telemetry::LokiLogService;
use reinhardt_cloud_types::log::LogFilter;
use rstest::rstest;
use testcontainers::GenericImage;
use testcontainers::core::IntoContainerPort;
use testcontainers::runners::AsyncRunner;
use tokio_stream::StreamExt;

/// Start a single-node Loki container and return `(container, base_url)`.
async fn start_loki() -> (testcontainers::ContainerAsync<GenericImage>, String) {
	let container = GenericImage::new("grafana/loki", "3.2.1")
		.with_exposed_port(3100.tcp())
		// No stdout wait: the boot banner string varies. Rely on `wait_ready`
		// polling `/ready` instead (Loki reports ready once the ring forms).
		.start()
		.await
		.expect("loki container started");
	let port = container
		.get_host_port_ipv4(3100.tcp())
		.await
		.expect("loki port mapped");
	let endpoint = format!("http://127.0.0.1:{port}");
	wait_ready(&endpoint).await;
	(container, endpoint)
}

/// Poll Loki's `/ready` endpoint until it responds 2xx (max ~30s).
async fn wait_ready(endpoint: &str) {
	let client = reqwest::Client::new();
	let deadline = std::time::Instant::now() + Duration::from_secs(45);
	while std::time::Instant::now() < deadline {
		if let Ok(resp) = client.get(format!("{endpoint}/ready")).send().await {
			if resp.status().is_success() {
				return;
			}
		}
		tokio::time::sleep(Duration::from_millis(500)).await;
	}
	panic!("loki never became ready at {endpoint}/ready");
}

/// Push a single log line to Loki for the given app label.
async fn push_line(endpoint: &str, app: &str, line: &str) {
	let ts_ns = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
	let payload = serde_json::json!({
		"streams": [{
			"stream": { "app": app, "level": "info" },
			"values": [[ts_ns.to_string(), line]]
		}]
	});
	reqwest::Client::new()
		.post(format!("{endpoint}/loki/api/v1/push"))
		.header("Content-Type", "application/json")
		.body(payload.to_string())
		.send()
		.await
		.expect("push accepted")
		.error_for_status()
		.expect("push 2xx");
}

#[rstest]
#[tokio::test]
async fn list_returns_pushed_line() {
	// Arrange
	let (_container, endpoint) = start_loki().await;
	push_line(&endpoint, "p-list", "hello-list").await;
	// Give Loki a moment to index the pushed line.
	tokio::time::sleep(Duration::from_secs(1)).await;
	let svc = LokiLogService::new(&endpoint);

	// Act
	let resp = svc
		.list_logs(
			LogFilter {
				source: Some("p-list".to_string()),
				..Default::default()
			},
			PaginationParams::new(Some(1), Some(50)),
		)
		.await
		.expect("list ok");

	// Assert
	assert!(
		resp.items.iter().any(|e| e.message == "hello-list"),
		"expected pushed line in {:?}",
		resp.items
	);
}

#[rstest]
#[tokio::test]
async fn tail_yields_pushed_line() {
	// Arrange
	let (_container, endpoint) = start_loki().await;
	let svc = LokiLogService::new(&endpoint);

	// Start tailing before pushing.
	let mut tail = svc
		.tail_logs(LogFilter {
			source: Some("p-tail".to_string()),
			..Default::default()
		})
		.await
		.expect("tail ok");
	tokio::time::sleep(Duration::from_millis(500)).await;
	push_line(&endpoint, "p-tail", "hello-tail").await;

	// Act — first entry within a timeout. Retry the push in case the tail
	// subscription was not yet active when the first line landed.
	let entry = tokio::time::timeout(Duration::from_secs(10), async {
		let mut interval = tokio::time::interval(Duration::from_millis(750));
		interval.tick().await; // first tick is immediate
		loop {
			tokio::select! {
				next = tail.next() => match next {
					Some(Ok(entry)) => return entry,
					Some(Err(e)) => panic!("tail stream returned error: {e}"),
					None => panic!("tail stream ended before yielding a log entry"),
				},
				_ = interval.tick() => {
					// Re-push in case the subscription raced past the first line.
					push_line(&endpoint, "p-tail", "hello-tail").await;
				}
			}
		}
	})
	.await
	.expect("tail yielded within timeout");

	// Assert
	assert_eq!(entry.message, "hello-tail");
}
