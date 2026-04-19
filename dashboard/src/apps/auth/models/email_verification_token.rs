//! Email verification token model.
//!
//! Tracks pending email-change requests. The plaintext token is never
//! persisted — only the SHA-256 hash (hex-encoded, 64 chars) is stored.
//!
//! Verification security model:
//! The verification endpoint hashes the submitted plaintext and looks up the
//! resulting hash in the database using an equality filter. Because the lookup
//! is by exact hash match (not by scanning a candidate list), timing depends on
//! the uniform index lookup rather than on token position in a list. A
//! constant-time comparison at the application layer would add no additional
//! security here: the token hash is globally unique (UNIQUE constraint), so
//! there is at most one candidate row, and the timing of a DB index lookup does
//! not reveal anything useful to an attacker. Application-level constant-time
//! comparison would only be relevant if we were scanning multiple rows in
//! memory — which we do not.

use chrono::{DateTime, Utc};
use reinhardt::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Pending email-change verification token.
///
/// Security properties:
/// - `token_hash` stores SHA-256(plaintext) as lowercase hex; the plaintext
///   is delivered by email and never written to the database.
/// - `expires_at` bounds the validity window (typically 24 hours).
/// - `consumed_at` marks single-use semantics; once set, the token cannot be
///   reused even within the validity window.
#[derive(Default, Serialize, Deserialize)]
#[model(app_label = "auth", table_name = "auth_email_verification_tokens")]
pub struct EmailVerificationToken {
	/// Primary key (None for auto-increment on insert).
	#[field(primary_key = true)]
	pub id: Option<i64>,

	/// Owning user (foreign key to `auth_users.id`).
	pub user_id: Uuid,

	/// New email that will replace `user.email` upon successful verification.
	#[field(max_length = 254)]
	pub pending_email: String,

	/// SHA-256(plaintext token) encoded as lowercase hex (64 chars).
	#[field(max_length = 64, unique = true)]
	pub token_hash: String,

	/// Token expiry timestamp (UTC).
	pub expires_at: DateTime<Utc>,

	/// Set when the token is used. `None` means still pending.
	pub consumed_at: Option<DateTime<Utc>>,

	/// Creation timestamp.
	#[field(auto_now_add = true)]
	pub created_at: DateTime<Utc>,
}
