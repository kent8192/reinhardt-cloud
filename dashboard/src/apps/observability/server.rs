//! Process liveness without credentials or organization data.

use reinhardt::{Request, Response, get};

#[get("/healthz/", name = "healthz")]
pub async fn healthz(_request: Request) -> reinhardt::http::Result<Response> {
	Response::ok().with_json(&serde_json::json!({
		"status": "ok",
		"service": "reinhardt-cloud",
		"version": env!("CARGO_PKG_VERSION"),
	}))
}
