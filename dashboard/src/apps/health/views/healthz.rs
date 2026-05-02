//! Unauthenticated `/api/healthz/` endpoint for Kubernetes probes.
//!
//! Performs two lightweight probes:
//!
//! 1. A database probe via `User::objects().count()` — exercises the
//!    globally configured `DatabaseConnection` with a cheap `SELECT COUNT(*)`
//!    that proves the connection is open and the schema is reachable.
//! 2. A gRPC probe via the standard `grpc.health.v1.Health/Check` RPC,
//!    using the shared `GrpcChannelSingleton` so the probe does not
//!    establish a new TCP connection on every call.
//!
//! Each probe is wrapped in a short timeout so a hung dependency cannot
//! wedge the liveness check and prevent Kubernetes from restarting the
//! pod. Any probe failure downgrades the overall status to `error` and
//! the endpoint responds with HTTP 503.

use std::time::Duration;

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::Model;
use reinhardt::di::Depends;
use reinhardt::http::ViewResult;
use reinhardt::{Response, StatusCode, get};
use tokio::time::timeout;
use tonic_health::pb::HealthCheckRequest;
use tonic_health::pb::health_client::HealthClient;
use tracing::warn;

use crate::apps::auth::models::User;
use crate::apps::health::serializers::{HealthzResponse, STATUS_ERROR, STATUS_OK};
use crate::config::grpc_client::GrpcChannelSingleton;

/// Per-probe timeout. Each individual probe (DB and gRPC) must complete
/// within this window or it is reported as `"error"`.
///
/// Two seconds is generous enough to absorb routine scheduling jitter
/// but tight enough that a hung dependency still fails probes quickly.
const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Probe the database by running a cheap `COUNT(*)` against an existing
/// table via the ORM.
///
/// Returns `true` on success, `false` on any failure (including timeout).
async fn probe_database() -> bool {
	match timeout(PROBE_TIMEOUT, User::objects().count()).await {
		Ok(Ok(_)) => true,
		Ok(Err(err)) => {
			warn!("healthz: database probe failed: {err}");
			false
		}
		Err(_) => {
			warn!("healthz: database probe timed out after {PROBE_TIMEOUT:?}");
			false
		}
	}
}

/// Probe the gRPC channel by issuing an empty `grpc.health.v1.Health/Check`
/// call, which returns the overall server `SERVING` state.
///
/// An empty `service` field is the standard way to ask for the server-wide
/// health status under the gRPC Health Checking Protocol.
async fn probe_grpc(grpc_channel: &GrpcChannelSingleton) -> bool {
	let mut client = HealthClient::new(grpc_channel.channel.clone());
	let request = HealthCheckRequest {
		service: String::new(),
	};
	match timeout(PROBE_TIMEOUT, client.check(request)).await {
		Ok(Ok(_)) => true,
		Ok(Err(err)) => {
			warn!("healthz: gRPC probe failed: {err}");
			false
		}
		Err(_) => {
			warn!("healthz: gRPC probe timed out after {PROBE_TIMEOUT:?}");
			false
		}
	}
}

/// Render a probe boolean as its stable status string.
fn status_str(ok: bool) -> String {
	if ok {
		STATUS_OK.to_string()
	} else {
		STATUS_ERROR.to_string()
	}
}

/// GET `/api/healthz/` — Kubernetes-friendly liveness and readiness probe.
///
/// Returns HTTP 200 with `{"status":"ok","db":"ok","grpc":"ok"}` when
/// all probes succeed. Returns HTTP 503 with the failing probes set to
/// `"error"` otherwise. The response shape is stable and safe to consume
/// from external monitoring tools.
///
/// This endpoint is intentionally exempt from session authentication (see
/// `config::urls::create_cookie_session_config`) so kubelet probes do
/// not require credentials.
#[get("/healthz/", name = "healthz")]
pub async fn healthz(
	#[inject] grpc_channel: Depends<GrpcChannelSingleton>,
) -> ViewResult<Response> {
	let db_ok = probe_database().await;
	let grpc_ok = probe_grpc(&grpc_channel).await;

	let all_ok = db_ok && grpc_ok;
	let http_status = if all_ok {
		StatusCode::OK
	} else {
		StatusCode::SERVICE_UNAVAILABLE
	};

	let body = HealthzResponse {
		status: status_str(all_ok),
		db: status_str(db_ok),
		grpc: status_str(grpc_ok),
	};
	let bytes = json::to_vec(&body)
		.map_err(|e| AppError::Internal(format!("Failed to serialize healthz response: {e}")))?;

	Ok(Response::new(http_status)
		.with_header("Content-Type", "application/json")
		.with_body(bytes))
}
