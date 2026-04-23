//! Response serializers for the `/api/healthz/` endpoint.

use reinhardt::{Schema, ToSchema};
use serde::Serialize;

/// Status string for an individual health check probe or the
/// overall dashboard status.
///
/// The string values `"ok"` and `"error"` are stable and should be
/// treated as part of the public `/healthz/` contract; Kubernetes
/// probes and external monitoring tooling may match against them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
	/// The probed subsystem responded successfully.
	Ok,
	/// The probed subsystem failed to respond successfully.
	Error,
}

/// Aggregated health check response returned by `/api/healthz/`.
///
/// The top-level `status` is `ok` only when every individual probe
/// (DB, gRPC) reports `ok`. Otherwise it is `error` and the endpoint
/// responds with HTTP 503.
#[derive(Debug, Serialize, Schema)]
pub struct HealthzResponse {
	/// Aggregated overall dashboard status.
	pub status: HealthStatus,
	/// Database connection probe result.
	pub db: HealthStatus,
	/// gRPC channel probe result.
	pub grpc: HealthStatus,
}
