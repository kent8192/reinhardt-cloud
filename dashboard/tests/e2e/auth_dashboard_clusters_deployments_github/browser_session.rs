//! Browser sessions whose transport and teardown outlive a cancelled test future.

use std::ops::Deref;
use std::time::Duration;

use anyhow::{Context, Result};
use reinhardt_test::{BrowserClient, BrowserConfig};
use tokio::runtime::{Builder, Runtime};

/// Own the WebDriver transport runtime until session deletion has completed.
pub(super) struct BrowserSession {
	session: Option<(BrowserClient, Runtime)>,
}

impl BrowserSession {
	pub(super) async fn connect(config: BrowserConfig) -> Result<Self> {
		tokio::task::spawn_blocking(move || {
			let runtime = Builder::new_multi_thread()
				.worker_threads(1)
				.enable_all()
				.build()
				.context("failed to create browser transport runtime")?;
			let client = runtime
				.block_on(BrowserClient::connect(config))
				.context("failed to connect browser session")?;
			Ok(Self {
				session: Some((client, runtime)),
			})
		})
		.await
		.context("browser connection worker failed")?
	}
}

impl Deref for BrowserSession {
	type Target = BrowserClient;

	fn deref(&self) -> &Self::Target {
		&self
			.session
			.as_ref()
			.expect("session is owned until Drop")
			.0
	}
}

impl Drop for BrowserSession {
	fn drop(&mut self) {
		let Some((client, runtime)) = self.session.take() else {
			return;
		};
		// Keep the transport worker alive independently of the test's runtime.
		// A separate thread also permits runtime shutdown during async unwinding.
		let cleanup = std::thread::spawn(move || {
			runtime.block_on(async {
				tokio::time::timeout(Duration::from_secs(10), client.close())
					.await
					.context("browser session deletion timed out")?
					.context("browser session deletion failed")
			})
		})
		.join();
		match cleanup {
			Ok(Ok(())) => {}
			Ok(Err(error)) => eprintln!("Browser teardown failed: {error:#}"),
			Err(_) => eprintln!("Browser teardown worker panicked"),
		}
	}
}

#[cfg(test)]
mod tests {
	use std::panic::{AssertUnwindSafe, catch_unwind};

	use reinhardt_test::BrowserConfig;
	use rstest::rstest;
	use serde_json::json;
	use wiremock::matchers::{method, path};
	use wiremock::{Mock, MockServer, ResponseTemplate};

	use super::BrowserSession;

	async fn webdriver() -> MockServer {
		let server = MockServer::start().await;
		Mock::given(method("POST"))
			.and(path("/session"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({
				"value": {
					"sessionId": "dashboard-test-session",
					"capabilities": {"browserName": "chrome"}
				}
			})))
			.mount(&server)
			.await;
		Mock::given(method("DELETE"))
			.and(path("/session/dashboard-test-session"))
			.respond_with(ResponseTemplate::new(200).set_body_json(json!({"value": null})))
			.expect(1)
			.mount(&server)
			.await;
		server
	}

	#[rstest]
	#[case::normal_return(false)]
	#[case::panic(true)]
	#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
	async fn session_is_deleted_when_its_guard_leaves_scope(#[case] panic: bool) {
		// Arrange
		let server = webdriver().await;
		let browser = BrowserSession::connect(BrowserConfig {
			webdriver_url: server.uri(),
			..BrowserConfig::default()
		})
		.await
		.expect("connect guarded session");

		// Act
		let outcome = catch_unwind(AssertUnwindSafe(move || {
			let _browser = browser;
			assert!(!panic, "simulated browser assertion failure");
		}));

		// Assert
		assert_eq!(outcome.is_err(), panic);
		server.verify().await;
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
	async fn session_is_deleted_when_the_test_future_is_cancelled() {
		// Arrange
		let server = webdriver().await;
		let browser = BrowserSession::connect(BrowserConfig {
			webdriver_url: server.uri(),
			..BrowserConfig::default()
		})
		.await
		.expect("connect guarded session");
		let (started, ready) = tokio::sync::oneshot::channel();
		let task = tokio::spawn(async move {
			let _browser = browser;
			started.send(()).expect("test is waiting");
			std::future::pending::<()>().await;
		});
		ready.await.expect("session is held by the task");

		// Act
		task.abort();
		let outcome = task.await;

		// Assert
		assert_eq!(outcome.expect_err("cancelled task").is_cancelled(), true);
		server.verify().await;
	}
}
