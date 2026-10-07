//! Identity validation independent of a browser or Git provider.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OidcIdentity {
	pub issuer: String,
	pub subject: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionError {
	Expired,
	Revoked,
	InactiveUser,
}

pub struct SessionFacts {
	pub user_id: Uuid,
	pub expires_at: DateTime<Utc>,
	pub revoked: bool,
	pub user_active: bool,
}

impl SessionFacts {
	pub fn authenticate(&self, now: DateTime<Utc>) -> Result<Uuid, SessionError> {
		if self.revoked {
			return Err(SessionError::Revoked);
		}
		if !self.user_active {
			return Err(SessionError::InactiveUser);
		}
		if now >= self.expires_at {
			return Err(SessionError::Expired);
		}
		Ok(self.user_id)
	}
}
