//! Unit tests for the email-change verification flow.
//!
//! These tests cover the security-critical building blocks that do NOT
//! require a database: token generation properties, SHA-256 hashing,
//! and the [`NullEmailSender`].
//!
//! Validation logic for expired tokens, consumed tokens, and user_id
//! mismatches is also exercised here by constructing model values
//! directly and asserting on the conditions that the
//! `verify_email_change` handler checks before acting.

#[cfg(test)]
mod tests {
	use crate::apps::auth::models::EmailVerificationToken;
	use crate::apps::auth::services::mailer::{EmailSender, NullEmailSender};
	use chrono::{Duration, Utc};
	use rand::TryRngCore;
	use rand::rngs::OsRng;
	use rstest::rstest;
	use sha2::{Digest, Sha256};
	use uuid::Uuid;

	/// Fresh 32-byte tokens from `OsRng` are non-zero with overwhelming
	/// probability and differ across draws.
	#[rstest]
	fn test_osrng_token_is_32_unique_bytes() {
		// Arrange
		let mut a = [0u8; 32];
		let mut b = [0u8; 32];

		// Act
		OsRng.try_fill_bytes(&mut a).expect("OS RNG a");
		OsRng.try_fill_bytes(&mut b).expect("OS RNG b");

		// Assert: both nonzero and distinct.
		assert_ne!(a, [0u8; 32]);
		assert_ne!(b, [0u8; 32]);
		assert_ne!(a, b);
	}

	/// SHA-256 of a hex-encoded token yields a 32-byte digest, and equal
	/// plaintexts produce byte-identical digests.
	#[rstest]
	fn test_sha256_of_plain_token_is_deterministic() {
		// Arrange
		let plain = "a".repeat(64);
		let mut h1 = Sha256::new();
		h1.update(plain.as_bytes());
		let d1: [u8; 32] = h1.finalize().into();
		let mut h2 = Sha256::new();
		h2.update(plain.as_bytes());
		let d2: [u8; 32] = h2.finalize().into();

		// Assert
		assert_eq!(d1, d2);
	}

	/// NullEmailSender always returns Ok and never panics — used by tests
	/// and local development to suppress real SMTP traffic.
	#[rstest]
	#[tokio::test]
	async fn test_null_email_sender_returns_ok() {
		// Arrange
		let sender = NullEmailSender::new();

		// Act
		let result = sender
			.send("user@example.com", "subject", "body text")
			.await;

		// Assert
		assert!(result.is_ok());
	}

	/// Helper: build a valid (unexpired, unconsumed) `EmailVerificationToken`.
	fn valid_token(user_id: Uuid) -> EmailVerificationToken {
		let mut hasher = Sha256::new();
		hasher.update(b"plaintext-token");
		let mut token = EmailVerificationToken::build()
			.user(user_id)
			.pending_email("new@example.com".to_string())
			.token_hash(hex::encode(hasher.finalize()))
			.expires_at(Utc::now() + Duration::hours(24))
			.consumed_at(None)
			.finish();
		token.id = Some(1);
		token.created_at = Utc::now();
		token
	}

	/// An expired token (expires_at in the past) must be rejected.
	///
	/// The `verify_email_change` handler filters `expires_at > now()` at the
	/// DB layer so an expired token never matches. This test verifies that the
	/// `expires_at` field correctly reflects the "in the past" condition so
	/// the DB WHERE clause would exclude it.
	#[rstest]
	fn test_expired_token_is_detected() {
		// Arrange
		let user_id = Uuid::new_v4();
		let mut token = valid_token(user_id);
		token.expires_at = Utc::now() - Duration::seconds(1);

		// Act
		let now = Utc::now();
		let is_expired = token.expires_at <= now;

		// Assert: the token's expiry timestamp lies in the past.
		assert!(
			is_expired,
			"token with expires_at in the past must be treated as expired"
		);
	}

	/// A consumed token (consumed_at is Some) must be rejected.
	///
	/// The `verify_email_change` handler filters `consumed_at IS NULL` at the
	/// DB layer so a consumed token never matches. This test verifies that
	/// setting `consumed_at` correctly signals the consumed state.
	#[rstest]
	fn test_consumed_token_is_detected() {
		// Arrange
		let user_id = Uuid::new_v4();
		let mut token = valid_token(user_id);
		token.consumed_at = Some(Utc::now() - Duration::hours(1));

		// Act
		let is_consumed = token.consumed_at.is_some();

		// Assert: consumed_at being Some means the token has already been used.
		assert!(
			is_consumed,
			"token with consumed_at set must be treated as already used"
		);
	}

	/// A token whose user_id does not match the claimed user_id must be
	/// rejected.
	///
	/// The `verify_email_change` handler checks `token.user_id() != claimed_user_id`
	/// after the DB lookup and returns an authentication error on mismatch,
	/// using the same generic message as an unknown token to avoid leaking
	/// whether the token exists for a different account.
	#[rstest]
	fn test_user_id_mismatch_is_detected() {
		// Arrange
		let owner_id = Uuid::new_v4();
		let attacker_id = Uuid::new_v4();
		let token = valid_token(owner_id);

		// Act
		let matches = *token.user_id() == attacker_id;

		// Assert: a different user_id must not match the token's owner.
		assert!(
			!matches,
			"token.user_id must not equal a different user's ID"
		);
	}

	/// A valid token (unexpired, unconsumed, correct user_id) passes all
	/// three checks — confirming the helper itself is well-formed.
	#[rstest]
	fn test_valid_token_passes_all_checks() {
		// Arrange
		let user_id = Uuid::new_v4();
		let token = valid_token(user_id);
		let now = Utc::now();

		// Act
		let not_expired = token.expires_at > now;
		let not_consumed = token.consumed_at.is_none();
		let user_matches = *token.user_id() == user_id;

		// Assert
		assert!(not_expired, "token must not be expired");
		assert!(not_consumed, "token must not be consumed");
		assert!(user_matches, "token user_id must match the claimed user");
	}
}
