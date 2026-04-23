//! Cluster agent JWT token issuance helper.
//!
//! Mints a JWT containing `AgentClaims { cluster_id }` and computes an
//! Argon2id hash of the plaintext token for persistence. The plaintext
//! is returned exactly once to the caller and never persisted.

use reinhardt::Argon2Hasher;
use reinhardt::PasswordHasher;
use reinhardt::core::exception::Error as AppError;
use reinhardt_cloud_grpc::agent_claims::create_agent_token;
use uuid::Uuid;

/// Token expiry in hours for newly-issued cluster agent tokens.
///
/// 30 days is long enough to not require frequent rotations during
/// normal operation while short enough that leaked tokens lose value.
pub const AGENT_TOKEN_EXPIRY_HOURS: i64 = 24 * 30;

/// Result of minting an agent token.
///
/// The `plaintext` is returned to the caller once and NEVER persisted.
/// The `hash` is stored in the database for verification on subsequent
/// agent connections.
pub struct IssuedAgentToken {
	/// Raw JWT string — show this to the API caller exactly once.
	pub plaintext: String,
	/// Argon2id hash of `plaintext`, safe to store in the database.
	pub hash: String,
}

/// Load the JWT signing secret from the environment.
///
/// Matches the convention used by the auth app's `LocalAuthService`.
pub fn jwt_secret() -> Result<String, AppError> {
	std::env::var("REINHARDT_CLOUD_JWT_SECRET").map_err(|_| {
		AppError::Internal(
			"JWT secret not configured: set REINHARDT_CLOUD_JWT_SECRET env var".to_string(),
		)
	})
}

/// Mint a cluster agent JWT and return both the plaintext and its hash.
pub fn issue_agent_token(cluster_id: Uuid) -> Result<IssuedAgentToken, AppError> {
	let secret = jwt_secret()?;
	let plaintext = create_agent_token(cluster_id, secret.as_bytes(), AGENT_TOKEN_EXPIRY_HOURS)
		.map_err(|e| AppError::Internal(format!("Failed to mint agent token: {e}")))?;

	let hasher = Argon2Hasher::new();
	let hash = hasher
		.hash(&plaintext)
		.map_err(|e| AppError::Internal(format!("Failed to hash agent token: {e}")))?;

	Ok(IssuedAgentToken { plaintext, hash })
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	use serial_test::serial;

	const TEST_SECRET: &str = "test-secret-for-cluster-token-issuance";

	#[rstest]
	#[serial(env_jwt_secret)]
	fn test_issue_agent_token_roundtrip_hash_verifies() {
		// Arrange
		unsafe { std::env::set_var("REINHARDT_CLOUD_JWT_SECRET", TEST_SECRET) };
		let cluster_id = Uuid::now_v7();

		// Act
		let issued = issue_agent_token(cluster_id).unwrap();

		// Assert
		assert!(!issued.plaintext.is_empty());
		assert!(!issued.hash.is_empty());
		assert_ne!(issued.plaintext, issued.hash);

		let hasher = Argon2Hasher::new();
		let verified = hasher.verify(&issued.plaintext, &issued.hash).unwrap();
		assert!(verified);
	}

	#[rstest]
	#[serial(env_jwt_secret)]
	fn test_issued_token_contains_cluster_id_claim() {
		// Arrange
		unsafe { std::env::set_var("REINHARDT_CLOUD_JWT_SECRET", TEST_SECRET) };
		let cluster_id = Uuid::now_v7();

		// Act
		let issued = issue_agent_token(cluster_id).unwrap();
		let claims = reinhardt_cloud_grpc::agent_claims::verify_agent_token(
			&issued.plaintext,
			TEST_SECRET.as_bytes(),
		)
		.unwrap();

		// Assert
		assert_eq!(claims.cluster_id, cluster_id.to_string());
	}

	#[rstest]
	#[serial(env_jwt_secret)]
	fn test_issue_agent_token_fails_without_secret() {
		// Arrange
		unsafe { std::env::remove_var("REINHARDT_CLOUD_JWT_SECRET") };
		let cluster_id = Uuid::now_v7();

		// Act
		let result = issue_agent_token(cluster_id);

		// Assert
		assert!(result.is_err());
	}
}
