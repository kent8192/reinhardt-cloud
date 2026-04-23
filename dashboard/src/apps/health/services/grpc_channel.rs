//! gRPC channel singleton used by the health check endpoint.
//!
//! Workaround for kent8192/reinhardt-cloud#391: this is a local copy of
//! the dashboard-level `GrpcChannelSingleton` introduced by PR #392.
//! Remove this module once PR #392 is merged, switching the health
//! probe to consume the shared singleton via DI instead.
//!
//! Ideal implementation (without workaround):
//!   // crates/.../views/healthz.rs
//!   use crate::config::grpc_client::GrpcChannelSingleton;
//!
//!   #[get("/healthz/", name = "healthz")]
//!   pub async fn healthz(
//!       #[inject] grpc_channel: Depends<GrpcChannelSingleton>,
//!   ) -> ViewResult<Response> { ... }
//!
//! The singleton wraps a lazily-connected `tonic::transport::Channel`
//! so the health check probe does not pay the cost of establishing a
//! fresh TCP connection on every request.

use reinhardt::di::injectable_factory;
use tonic::transport::{Channel, Endpoint};

/// Default gRPC endpoint used when `GRPC_ENDPOINT` is not set.
///
/// Mirrors the default used by `apps::deployments::views::deployment_logs`
/// so the health probe targets the same server the deployment endpoints use.
const DEFAULT_GRPC_ENDPOINT: &str = "http://127.0.0.1:50051";

/// Resolve the gRPC endpoint from the environment or fall back to the default.
fn resolve_grpc_endpoint() -> String {
	std::env::var("GRPC_ENDPOINT").unwrap_or_else(|_| DEFAULT_GRPC_ENDPOINT.to_string())
}

/// Lazily-connected gRPC channel registered as a DI singleton.
///
/// The channel is built via `Endpoint::connect_lazy`, so singleton
/// construction does not perform any network I/O. Actual connections
/// are established the first time the channel is used, and tonic
/// transparently retries on failure.
#[derive(Clone)]
pub struct GrpcChannelSingleton {
	channel: Channel,
}

impl GrpcChannelSingleton {
	/// Build a new singleton pointing at the given endpoint URL.
	///
	/// Returns an error if the URL cannot be parsed by tonic. Network
	/// connectivity is not verified here — the channel is lazy.
	pub fn new(endpoint_url: &str) -> Result<Self, tonic::transport::Error> {
		let endpoint = Endpoint::from_shared(endpoint_url.to_string())?;
		Ok(Self {
			channel: endpoint.connect_lazy(),
		})
	}

	/// Borrow the underlying `Channel` for readiness probes and RPC calls.
	pub fn channel(&self) -> &Channel {
		&self.channel
	}
}

/// DI factory — registers `GrpcChannelSingleton` as a process-wide singleton.
///
/// Tests can override via `SingletonScope::set()` before resolution to
/// point the channel at a bespoke endpoint (for example, an ephemeral
/// `127.0.0.1:0` port bound to a test gRPC server).
#[injectable_factory(scope = "singleton")]
async fn create_grpc_channel_singleton() -> GrpcChannelSingleton {
	let endpoint_url = resolve_grpc_endpoint();
	GrpcChannelSingleton::new(&endpoint_url)
		.expect("Failed to build gRPC channel singleton: GRPC_ENDPOINT must be a valid URL")
}
