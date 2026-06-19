//! API token model for CLI control-plane authentication.
//!
//! Stores a SHA-256 hash of the plaintext token (never the plaintext itself).
//! The plaintext is shown to the operator exactly once at creation time.
//! Revocation is a soft delete via `revoked_at`; expiry is optional.
//!
//! Verification security model:
//! The authentication middleware hashes the submitted plaintext and looks up
//! the resulting hash in the database using an equality filter. Because the
//! lookup is by exact hash match (not by scanning a candidate list), timing
//! depends on the uniform index lookup rather than on token position in a
//! list. A constant-time comparison at the application layer would add no
//! additional security here: the token hash is globally unique (UNIQUE
//! constraint), so there is at most one candidate row, and the timing of a DB
//! index lookup does not reveal anything useful to an attacker.

use chrono::{DateTime, Utc};
use reinhardt::db::associations::ForeignKeyField;
use reinhardt::prelude::*;
use serde::{Deserialize, Serialize};

use super::User;

/// Long-lived, revocable API token tied to a `User`.
///
/// Security properties:
/// - `token_hash` stores SHA-256(plaintext) as lowercase hex (64 chars); the
///   plaintext is returned once at creation and never persisted.
/// - `prefix` is a non-secret display prefix (first chars of the plaintext) so
///   users can identify a token in listings without the full value.
/// - `revoked_at` / `expires_at` bound validity; both `None` means active and
///   long-lived (the default).
#[model(app_label = "auth", table_name = "auth_api_keys")]
#[derive(Default, Serialize, Deserialize)]
pub struct ApiKey {
	/// Primary key (None for auto-increment on insert).
	#[field(primary_key = true)]
	pub id: Option<i64>,

	/// Owner of this token.
	#[rel(foreign_key, related_name = "api_keys")]
	pub user: ForeignKeyField<User>,

	/// SHA-256(plaintext token) encoded as lowercase hex (64 chars). Globally unique.
	#[field(max_length = 64, unique = true)]
	pub token_hash: String,

	/// Human-readable label, e.g. "CI deploy token".
	#[field(max_length = 100)]
	pub label: String,

	/// Non-secret display prefix (first 12 chars of the plaintext).
	#[field(max_length = 16)]
	pub prefix: String,

	/// Creation timestamp.
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,

	/// Optional expiry. `None` means long-lived (the default).
	pub expires_at: Option<DateTime<Utc>>,

	/// Soft-revoke timestamp. `None` means active.
	pub revoked_at: Option<DateTime<Utc>>,

	/// Last successful verification (for listings/diagnostics).
	pub last_used_at: Option<DateTime<Utc>>,
}
