//! Browser sessions (SR-07, SR-08).
//!
//! A session is a server-side record in Redis, shared by every replica. The
//! browser holds only an unguessable token; Redis is keyed by the SHA-256 of
//! that token, so a Redis dump or `KEYS` listing yields no usable cookie.
//!
//! - **Rotation.** [`SessionService::create`] always issues a new token. Sign-in
//!   destroys the session the browser presented, so a token planted before
//!   sign-in (fixation) is worthless afterwards.
//! - **Lifetime.** A session expires after [`SessionPolicy::idle`] without use
//!   and [`SessionPolicy::absolute`] after creation, whichever comes first. The
//!   idle timer is Redis's own key expiry, refreshed (never beyond the absolute
//!   limit) each time the session is resolved.
//! - **No privileges.** The record names only the User. Whether the User is
//!   active, and whether they are Staff, is read from the database on every
//!   request (SR-07); nothing here can go stale.
//! - **Revocation.** Each User has an index of their live sessions, so every
//!   session of a User can be destroyed at once (deactivation, GitHub account
//!   re-pointing).

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use rand::Rng;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::apps::accounts::services::server::redis_handle::{RedisError, RedisHandle};

/// How long a session may live.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionPolicy {
	/// Time a session survives without being used.
	pub idle: Duration,
	/// Time after creation at which a session ends however active it is.
	pub absolute: Duration,
}

impl SessionPolicy {
	/// The limits SR-08 sets: 30 minutes idle, 24 hours in total.
	pub const STANDARD: Self = Self {
		idle: Duration::from_secs(30 * 60),
		absolute: Duration::from_secs(24 * 60 * 60),
	};
}

/// The secret a browser presents to prove its session.
///
/// Deliberately not `Debug`/`Display`: the only way out is
/// [`SessionToken::expose`], used when setting or matching the cookie.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionToken(String);

impl SessionToken {
	/// Wrap the value read from a cookie. It is not trusted until resolved.
	#[must_use]
	pub fn from_cookie(value: &str) -> Self {
		Self(value.to_owned())
	}

	/// The token text, for the `Set-Cookie` header.
	#[must_use]
	pub fn expose(&self) -> &str {
		&self.0
	}

	fn generate() -> Self {
		let mut bytes = [0_u8; 32];
		rand::rng().fill(&mut bytes);
		Self(URL_SAFE_NO_PAD.encode(bytes))
	}

	fn key_digest(&self) -> String {
		hex_lower(&Sha256::digest(self.0.as_bytes()))
	}
}

impl std::fmt::Debug for SessionToken {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("SessionToken(<redacted>)")
	}
}

/// A newly created session.
#[derive(Debug, Clone)]
pub struct IssuedSession {
	/// The token to set as the session cookie.
	pub token: SessionToken,
	/// The longest the session can live; the cookie's `Max-Age`.
	pub lifetime: Duration,
}

/// Failures of session storage. Details are for server logs only (SR-14).
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
	/// Redis failed.
	#[error(transparent)]
	Redis(#[from] RedisError),
	/// A stored record could not be read back.
	#[error("session record is malformed")]
	Malformed,
}

impl From<redis::RedisError> for SessionError {
	fn from(error: redis::RedisError) -> Self {
		Self::Redis(RedisError(error))
	}
}

#[derive(Serialize, Deserialize)]
struct Record {
	/// Internal ID of the signed-in User.
	user: Uuid,
	/// Creation time in milliseconds since the Unix epoch.
	created_ms: i64,
}

/// Redis-backed browser sessions.
#[derive(Clone, Debug)]
pub struct SessionService {
	redis: RedisHandle,
	policy: SessionPolicy,
	prefix: String,
}

impl SessionService {
	/// Create the service with the production limits.
	#[must_use]
	pub fn new(redis: RedisHandle) -> Self {
		Self::with_policy(redis, SessionPolicy::STANDARD)
	}

	/// Create the service with explicit limits.
	///
	/// Production code uses [`SessionService::new`]; shorter limits exist so
	/// tests can observe expiry.
	#[must_use]
	pub fn with_policy(redis: RedisHandle, policy: SessionPolicy) -> Self {
		Self {
			redis,
			policy,
			prefix: "cloud:session:".to_owned(),
		}
	}

	/// The lifetime limits in force.
	#[must_use]
	pub const fn policy(&self) -> SessionPolicy {
		self.policy
	}

	fn session_key(&self, digest: &str) -> String {
		format!("{}s:{digest}", self.prefix)
	}

	fn user_key(&self, user: Uuid) -> String {
		format!("{}u:{user}", self.prefix)
	}

	/// Create a session for `user` under a freshly generated token.
	///
	/// # Errors
	///
	/// Returns an error when Redis is unreachable or refuses the write.
	pub async fn create(&self, user: Uuid) -> Result<IssuedSession, SessionError> {
		let token = SessionToken::generate();
		let digest = token.key_digest();
		let record = serde_json::to_string(&Record {
			user,
			created_ms: Utc::now().timestamp_millis(),
		})
		.map_err(|_| SessionError::Malformed)?;
		let mut connection = self.redis.connection().await?;
		// One transaction, so an index entry never exists without its session.
		let () = redis::pipe()
			.atomic()
			.cmd("SET")
			.arg(self.session_key(&digest))
			.arg(record)
			.arg("PX")
			.arg(millis(self.policy.idle.min(self.policy.absolute)))
			.ignore()
			.cmd("SADD")
			.arg(self.user_key(user))
			.arg(&digest)
			.ignore()
			.cmd("PEXPIRE")
			.arg(self.user_key(user))
			.arg(millis(self.policy.absolute))
			.ignore()
			.query_async(&mut connection)
			.await?;
		Ok(IssuedSession {
			token,
			lifetime: self.policy.absolute,
		})
	}

	/// Start a session for `user` in place of the one the browser presented.
	///
	/// This is the one place that rotates a session (SR-08), shared by every way
	/// of signing in (the GitHub callback and Login Link consumption), so they
	/// cannot drift apart. `previous` is the session cookie the browser sent, if
	/// any: it is destroyed first, because a token planted before sign-in
	/// (fixation) must not survive it. When it cannot be destroyed it would stay
	/// valid beside the new session, so no session is issued at all.
	///
	/// # Errors
	///
	/// Returns an error when Redis is unreachable or refuses a write.
	pub async fn replace(
		&self,
		user: Uuid,
		previous: Option<&SessionToken>,
	) -> Result<IssuedSession, SessionError> {
		if let Some(previous) = previous {
			self.destroy(previous).await?;
		}
		self.create(user).await
	}

	/// The User a presented token belongs to, if the session is still alive.
	///
	/// Using a session slides its idle timer forward, but never past the
	/// absolute limit. A session past that limit is destroyed here.
	///
	/// # Errors
	///
	/// Returns an error when Redis is unreachable or a record is unreadable.
	pub async fn resolve(&self, token: &SessionToken) -> Result<Option<Uuid>, SessionError> {
		let digest = token.key_digest();
		let key = self.session_key(&digest);
		let mut connection = self.redis.connection().await?;
		let raw: Option<String> = connection.get(&key).await?;
		let Some(raw) = raw else {
			return Ok(None);
		};
		let record: Record = serde_json::from_str(&raw).map_err(|_| SessionError::Malformed)?;
		let age_ms = (Utc::now().timestamp_millis() - record.created_ms).max(0);
		let absolute_ms = millis(self.policy.absolute);
		if age_ms >= absolute_ms {
			let _: () = connection.del(&key).await?;
			let _: () = connection.srem(self.user_key(record.user), &digest).await?;
			return Ok(None);
		}
		let slid = millis(self.policy.idle).min(absolute_ms - age_ms);
		// `PEXPIRE` reports 0 when the key vanished since the read.
		let extended: bool = connection.pexpire(&key, slid).await?;
		Ok(extended.then_some(record.user))
	}

	/// Destroy the session behind `token` on the server.
	///
	/// Returns the User it belonged to, or `None` when there was no such
	/// session. Destroying a session twice is harmless.
	///
	/// # Errors
	///
	/// Returns an error when Redis is unreachable or a record is unreadable.
	pub async fn destroy(&self, token: &SessionToken) -> Result<Option<Uuid>, SessionError> {
		let digest = token.key_digest();
		let mut connection = self.redis.connection().await?;
		let raw: Option<String> = redis::cmd("GETDEL")
			.arg(self.session_key(&digest))
			.query_async(&mut connection)
			.await?;
		let Some(raw) = raw else {
			return Ok(None);
		};
		let record: Record = serde_json::from_str(&raw).map_err(|_| SessionError::Malformed)?;
		let _: () = connection.srem(self.user_key(record.user), &digest).await?;
		Ok(Some(record.user))
	}

	/// Destroy every session of `user`; returns how many were live.
	///
	/// # Errors
	///
	/// Returns an error when Redis is unreachable or refuses the write.
	pub async fn destroy_all_for_user(&self, user: Uuid) -> Result<usize, SessionError> {
		let mut connection = self.redis.connection().await?;
		let user_key = self.user_key(user);
		let digests: Vec<String> = connection.smembers(&user_key).await?;
		let mut destroyed = 0;
		for digest in &digests {
			let removed: usize = connection.del(self.session_key(digest)).await?;
			destroyed += removed;
		}
		let _: () = connection.del(&user_key).await?;
		Ok(destroyed)
	}
}

fn millis(duration: Duration) -> i64 {
	i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
}

fn hex_lower(bytes: &[u8]) -> String {
	use std::fmt::Write;
	bytes.iter().fold(String::new(), |mut out, byte| {
		let _ = write!(out, "{byte:02x}");
		out
	})
}
