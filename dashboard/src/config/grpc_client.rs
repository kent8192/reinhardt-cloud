//! Shared gRPC client channel for dashboard handlers.
//!
//! Exposes [`GrpcChannelSingleton`], a DI-registered wrapper around a
//! lazy [`tonic::transport::Channel`]. The channel is created via
//! [`Channel::from_shared`] + [`tonic::transport::Endpoint::connect_lazy`],
//! so construction never performs a TCP connect. This lets the dashboard
//! boot even when the operator's gRPC server is not yet reachable, and
//! amortises the per-request cost of establishing a new connection for
//! every inbound HTTP request (see reinhardt-cloud#392).
//!
//! Per-RPC failures (e.g. operator unreachable) surface to handlers as
//! `tonic::Status` with `Code::Unavailable` and are mapped to HTTP 503
//! by the existing view-layer error translation.

use reinhardt::di::FactoryOutput;
use tonic::transport::{Channel, Endpoint};

/// Default gRPC endpoint used when `GRPC_ENDPOINT` is not set.
const DEFAULT_GRPC_ENDPOINT: &str = "http://127.0.0.1:50051";

/// DI-managed singleton wrapper around a lazy gRPC [`Channel`].
///
/// Construction is infallible: [`Endpoint::connect_lazy`] defers the
/// actual transport connect until the first RPC. Clone the inner
/// [`Channel`] to hand it to a generated tonic client — channels
/// are cheap to clone and share an underlying connection pool.
#[derive(Clone)]
pub struct GrpcChannelSingleton {
	/// Lazily-connected tonic transport channel.
	pub channel: Channel,
}

#[reinhardt::di::injectable_key]
pub struct GrpcChannelSingletonKey;

impl GrpcChannelSingleton {
	/// Build a [`GrpcChannelSingleton`] for the given endpoint URI.
	///
	/// Returns an error if `endpoint` is not a valid URI. The channel
	/// itself is connected lazily on first RPC.
	pub fn new(endpoint: &str) -> Result<Self, tonic::transport::Error> {
		let channel = Endpoint::from_shared(endpoint.to_owned())?.connect_lazy();
		Ok(Self { channel })
	}
}

/// Resolve the gRPC endpoint from the environment, falling back to a
/// loopback default for local development.
fn resolve_endpoint() -> String {
	std::env::var("GRPC_ENDPOINT").unwrap_or_else(|_| DEFAULT_GRPC_ENDPOINT.to_string())
}

/// DI factory — auto-registers [`GrpcChannelSingleton`] as a singleton.
///
/// The channel is created via [`Endpoint::connect_lazy`], so this
/// factory never fails on unreachable endpoints. Tests can override
/// via `SingletonScope::set()` before resolution.
#[reinhardt::di::injectable(scope = "singleton")]
async fn create_grpc_channel_singleton()
-> FactoryOutput<GrpcChannelSingletonKey, GrpcChannelSingleton> {
	let endpoint = resolve_endpoint();
	FactoryOutput::new(
		GrpcChannelSingleton::new(&endpoint)
			.expect("GRPC_ENDPOINT must be a valid URI (e.g. http://127.0.0.1:50051)"),
	)
}
