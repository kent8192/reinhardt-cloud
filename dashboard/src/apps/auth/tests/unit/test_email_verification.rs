//! Unit tests for the email-change verification flow.
//!
//! These tests cover the security-critical building blocks that do NOT
//! require a database: token generation properties, SHA-256 hashing,
//! constant-time hash comparison, and the [`NullEmailSender`]. Full
//! request-level happy/expired/consumed/invalid-token coverage requires
//! a PostgreSQL TestContainer and is tracked as an e2e follow-up.

#[cfg(test)]
mod tests {
	use crate::apps::auth::services::mailer::{EmailSender, NullEmailSender};
	use rand::TryRngCore;
	use rand::rngs::OsRng;
	use rstest::rstest;
	use sha2::{Digest, Sha256};
	use subtle::ConstantTimeEq;

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

	/// Constant-time compare returns 1 for equal digests and 0 for
	/// different digests (invalid-token rejection path).
	#[rstest]
	fn test_constant_time_eq_rejects_mismatched_digest() {
		// Arrange
		let mut hasher = Sha256::new();
		hasher.update(b"secret-plaintext-token");
		let good: [u8; 32] = hasher.finalize().into();
		let mut hasher2 = Sha256::new();
		hasher2.update(b"different-plaintext");
		let bad: [u8; 32] = hasher2.finalize().into();

		// Act
		let eq_self = good.ct_eq(&good).unwrap_u8();
		let eq_diff = good.ct_eq(&bad).unwrap_u8();

		// Assert
		assert_eq!(eq_self, 1);
		assert_eq!(eq_diff, 0);
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
}
