//! A shared, lazily established Redis connection.
//!
//! Sessions, sign-in notices, and the sign-in state all live in Redis so that
//! every replica sees the same data (SR-04, SR-08). The handle is cheap to
//! clone; the connection is opened by the first command, so building the
//! application never needs a reachable Redis.

use std::sync::Arc;

use redis::Client;
use std::time::Duration;

use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use reinhardt::conf::settings::secret_types::SecretString;
use tokio::sync::OnceCell;

/// Longest a connection attempt may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
/// Longest a command may take. A request must fail, not hang, when Redis is
/// unreachable.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(3);

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
			.get_or_try_init(|| {
				let config = ConnectionManagerConfig::new()
					.set_number_of_retries(1)
					.set_connection_timeout(CONNECT_TIMEOUT)
					.set_response_timeout(RESPONSE_TIMEOUT);
				ConnectionManager::new_with_config(self.client.clone(), config)
			})
			.await?;
		Ok(manager.clone())
	}
}
