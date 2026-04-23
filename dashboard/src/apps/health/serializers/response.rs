//! Response serializers for the `/api/healthz/` endpoint.

use reinhardt::{Schema, ToSchema};
use serde::Serialize;

/// Status string emitted for a single probe and for the overall response.
///
/// Declared as a stable `&'static str` rather than an enum so it can
/// participate in the OpenAPI schema without a bespoke `ToSchema`
/// implementation. The values `"ok"` and `"error"` are part of the
/// public `/healthz/` contract and are matched by Kubernetes probes
/// and external monitoring tooling.
pub const STATUS_OK: &str = "ok";
/// Negative counterpart to `STATUS_OK`.
pub const STATUS_ERROR: &str = "error";

/// Aggregated health check response returned by `/api/healthz/`.
///
/// The top-level `status` is `ok` only when every individual probe
/// (DB, gRPC) reports `ok`. Otherwise it is `error` and the endpoint
/// responds with HTTP 503.
#[derive(Debug, Serialize, Schema)]
pub struct HealthzResponse {
	/// Aggregated overall dashboard status (`"ok"` or `"error"`).
	pub status: String,
	/// Database connection probe result (`"ok"` or `"error"`).
	pub db: String,
	/// gRPC channel probe result (`"ok"` or `"error"`).
	pub grpc: String,
}
