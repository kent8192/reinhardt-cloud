//! A shared, lazily established Redis connection.
//!
//! Sessions, sign-in notices, and the sign-in state all live in Redis so that
//! every replica sees the same data (SR-04, SR-08). The handle is cheap to
//! clone; the connection is opened by the first command, so building the
//! application never needs a reachable Redis.

use std::sync::Arc;

use redis::Client;
use redis::aio::ConnectionManager;
use reinhardt::conf::settings::secret_types::SecretString;
use tokio::sync::OnceCell;

/// Failures talking to Redis. The message is for server logs only (SR-14).
#[derive(Debug, thiserror::Error)]
#[error("redis operation failed: {0}")]
pub struct RedisError(#[from] pub redis::RedisError);

/// A cloneable Redis handle.
#[derive(Clone)]
pub struct RedisHandle {
	client: Client,
	connection: Arc<OnceCell<ConnectionManager>>,
}

impl std::fmt::Debug for RedisHandle {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		// The URL carries the Redis password, so it is never printed.
		f.debug_struct("RedisHandle").finish_non_exhaustive()
	}
}

impl RedisHandle {
	/// Create a handle for `url`. No connection is made yet.
	///
	/// # Errors
	///
	/// Returns an error when `url` is not a valid Redis URL.
	pub fn new(url: &SecretString) -> Result<Self, RedisError> {
		Ok(Self {
			client: Client::open(url.expose_secret())?,
			connection: Arc::new(OnceCell::new()),
		})
	}

	/// A connection ready for commands, establishing it on first use.
	///
	/// # Errors
	///
	/// Returns an error when Redis cannot be reached.
	pub async fn connection(&self) -> Result<ConnectionManager, RedisError> {
		let manager = self
			.connection
			.get_or_try_init(|| ConnectionManager::new(self.client.clone()))
			.await?;
		Ok(manager.clone())
	}
}
