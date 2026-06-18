//! Loki `/loki/api/v1/tail` WebSocket consumer.

use std::pin::Pin;

use futures::StreamExt;
use reinhardt_cloud_core::error::ApiError;
use reinhardt_cloud_types::log::{LogEntry, LogFilter};
use tokio_stream::Stream;
use tokio_tungstenite::tungstenite::Message;

use super::LokiLogService;
use super::parse::parse_tail_frame;

/// Build the Loki tail URL from the base endpoint, deriving the WebSocket
/// scheme from the endpoint so an insecure scheme is never hard-coded: TLS
/// (`https`) endpoints use the secure WebSocket scheme; in-cluster plaintext
/// (`http`) endpoints use the plaintext WebSocket scheme.
fn tail_url(endpoint: &str, logql: &str) -> Result<String, ApiError> {
	let mut url = reqwest::Url::parse(&format!(
		"{}/loki/api/v1/tail",
		endpoint.trim_end_matches('/')
	))
	.map_err(|e| ApiError::Internal(format!("invalid loki endpoint: {e}")))?;
	url.query_pairs_mut().append_pair("query", logql);
	// Map the HTTP scheme to the matching WebSocket scheme via `set_scheme`,
	// which preserves host/path/query. `https` maps to the secure scheme; `http`
	// maps to the plaintext scheme. Both are special-scheme family changes, so
	// `set_scheme` succeeds.
	let ws_scheme = match url.scheme() {
		"https" => "wss",
		"http" => "ws",
		other => {
			return Err(ApiError::Internal(format!(
				"unsupported loki scheme: {other}"
			)));
		}
	};
	let _ = url.set_scheme(ws_scheme);
	Ok(url.as_str().to_string())
}

/// Tail matching log entries over the Loki WebSocket.
///
/// Opens `/loki/api/v1/tail?query=<LogQL>` and yields each tailed entry as a
/// `LogEntry`. Socket errors are yielded as `ApiError::Internal` and end the
/// stream; reconnection-with-backoff is a follow-up (see spec non-goal).
pub(super) async fn tail_logs(
	svc: &LokiLogService,
	filter: LogFilter,
) -> Result<Pin<Box<dyn Stream<Item = Result<LogEntry, ApiError>> + Send>>, ApiError> {
	let logql = super::query::build_logql(&filter);
	let url = tail_url(&svc.endpoint, &logql)?;

	let (mut ws, _resp) = tokio_tungstenite::connect_async(url)
		.await
		.map_err(|e| ApiError::Internal(format!("loki tail websocket failed: {e}")))?;

	let stream = async_stream::stream! {
		while let Some(msg) = ws.next().await {
			match msg {
				Ok(Message::Text(text)) => match parse_tail_frame(&text) {
					Ok(entries) => {
						for entry in entries {
							yield Ok(entry);
						}
					}
					Err(e) => {
						yield Err(ApiError::Internal(format!("loki tail parse failed: {e}")));
						break;
					}
				},
				Ok(_) => continue,
				Err(e) => {
					yield Err(ApiError::Internal(format!("loki tail socket error: {e}")));
					break;
				}
			}
		}
	};

	Ok(Box::pin(stream))
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn tail_url_maps_http_to_plaintext_ws_scheme() {
		// Arrange + Act
		let url = tail_url("http://loki:3100", r#"{app="p"}"#).unwrap();
		let parsed = reqwest::Url::parse(&url).unwrap();

		// Assert — host/path/query preserved, scheme downgraded for plaintext.
		assert_eq!(parsed.scheme(), "ws");
		assert_eq!(parsed.host_str(), Some("loki"));
		assert_eq!(parsed.port(), Some(3100));
		assert_eq!(parsed.path(), "/loki/api/v1/tail");
		assert!(parsed.query().unwrap_or("").contains("query="));
	}

	#[rstest]
	fn tail_url_maps_https_to_secure_ws_scheme() {
		// Arrange + Act
		let url = tail_url("https://loki.example:3100", r#"{app="p"}"#).unwrap();
		let parsed = reqwest::Url::parse(&url).unwrap();

		// Assert — TLS endpoint uses the secure WebSocket scheme.
		assert_eq!(parsed.scheme(), "wss");
		assert_eq!(parsed.host_str(), Some("loki.example"));
	}

	#[rstest]
	fn tail_url_rejects_unsupported_scheme() {
		// Arrange + Act
		let result = tail_url("ftp://loki:3100", "{app=\"p\"}");

		// Assert
		assert!(matches!(result, Err(ApiError::Internal(_))));
	}
}
