//! One-shot notices shown on the sign-in page after a failed sign-in.
//!
//! The GitHub callback is a browser navigation, so its outcome reaches the
//! sign-in page through the browser: the callback stores a notice in Redis,
//! keyed by an unguessable ID that only that browser receives in a short-lived
//! `HttpOnly` cookie, and the page redeems it once through a server function.
//! The GitHub login therefore never travels in a URL, where anyone could forge
//! a link that makes the page address a visitor with text of their choosing.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::apps::accounts::services::server::redis_handle::RedisHandle;
use crate::apps::accounts::services::server::sessions::SessionError;

/// How long an unredeemed notice survives.
pub const NOTICE_LIFETIME: Duration = Duration::from_secs(5 * 60);

/// What went wrong, in terms the page can phrase.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoticeKind {
	/// The sign-up policy did not admit the GitHub account.
	NotInvited,
	/// The sign-in was refused for a reason the page does not explain
	/// (deactivated account, invalid or expired state, GitHub error).
	Failed,
}

/// A stored notice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredNotice {
	/// The kind of notice.
	pub kind: NoticeKind,
	/// GitHub login of the visitor, for notices that address them by name.
	pub login: Option<String>,
}

/// Redis-backed one-shot notices.
#[derive(Clone, Debug)]
pub struct NoticeStore {
	redis: RedisHandle,
}

impl NoticeStore {
	/// Create the store.
	#[must_use]
	pub fn new(redis: RedisHandle) -> Self {
		Self { redis }
	}

	/// Store `notice` and return the ID to hand to the visitor's browser.
	///
	/// # Errors
	///
	/// Returns an error when Redis is unreachable or refuses the write.
	pub async fn put(&self, notice: &StoredNotice) -> Result<String, SessionError> {
		let mut bytes = [0_u8; 24];
		rand::rng().fill(&mut bytes);
		let id = URL_SAFE_NO_PAD.encode(bytes);
		let payload = serde_json::to_string(notice).map_err(|_| SessionError::Malformed)?;
		let mut connection = self.redis.connection().await?;
		let () = redis::cmd("SET")
			.arg(key(&id))
			.arg(payload)
			.arg("EX")
			.arg(NOTICE_LIFETIME.as_secs())
			.query_async(&mut connection)
			.await?;
		Ok(id)
	}

	/// Redeem a notice exactly once; a second call returns `None`.
	///
	/// # Errors
	///
	/// Returns an error when Redis is unreachable.
	pub async fn take(&self, id: &str) -> Result<Option<StoredNotice>, SessionError> {
		let mut connection = self.redis.connection().await?;
		let raw: Option<String> = redis::cmd("GETDEL")
			.arg(key(id))
			.query_async(&mut connection)
			.await?;
		Ok(raw.and_then(|raw| serde_json::from_str(&raw).ok()))
	}
}

fn key(id: &str) -> String {
	format!("cloud:signin-notice:{id}")
}
