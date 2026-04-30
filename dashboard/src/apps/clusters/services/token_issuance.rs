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

/// Load the JWT signing secret.
///
/// Resolves via `crate::config::settings::get_jwt_secret()`, which reads the
/// top-level `jwt_secret` key from the active TOML profile and falls back to
/// the `REINHARDT_CLOUD_JWT_SECRET` environment variable. Returns an error
/// only when neither source supplies a value.
///
/// Issue: kent8192/reinhardt-cloud#494
pub fn jwt_secret() -> Result<String, AppError> {
	crate::config::settings::get_jwt_secret().ok_or_else(|| {
		AppError::Internal(
			"JWT secret not configured: set jwt_secret in TOML or REINHARDT_CLOUD_JWT_SECRET env var"
				.to_string(),
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

	/// Path that is guaranteed not to contain settings TOML files.
	/// Used to neutralise TOML lookup so env-var resolution can be tested
	/// in isolation (`get_jwt_secret` falls back when TOML is absent).
	const NONEXISTENT_CONFIG_DIR: &str = "/nonexistent-test-config-dir-494";

	#[rstest]
	#[serial(env_jwt_secret)]
	fn test_issue_agent_token_roundtrip_hash_verifies() {
		// Arrange — secret is sourced from the active settings (TOML
		// or env var); test only verifies that the Argon2 hash matches
		// the issued plaintext, independent of the secret value.
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
		// Arrange — verify with the same secret that the function used
		// to mint the token, so the assertion holds regardless of whether
		// the secret came from TOML or the env-var fallback.
		let cluster_id = Uuid::now_v7();

		// Act
		let issued = issue_agent_token(cluster_id).unwrap();
		let secret = jwt_secret().unwrap();
		let claims = reinhardt_cloud_grpc::agent_claims::verify_agent_token(
			&issued.plaintext,
			secret.as_bytes(),
		)
		.unwrap();

		// Assert
		assert_eq!(claims.cluster_id, cluster_id.to_string());
	}

	#[rstest]
	#[serial(env_jwt_secret)]
	fn test_issue_agent_token_fails_without_secret() {
		// Arrange — neutralise both resolution sources: redirect TOML
		// lookup to a non-existent dir (so `base.toml`/`local.toml` cannot
		// be parsed) and remove the env-var fallback. Issue: #494.
		let prior_dir = std::env::var("REINHARDT_CLOUD_CONFIG_DIR").ok();
		let prior_secret = std::env::var("REINHARDT_CLOUD_JWT_SECRET").ok();
		unsafe { std::env::set_var("REINHARDT_CLOUD_CONFIG_DIR", NONEXISTENT_CONFIG_DIR) };
		unsafe { std::env::remove_var("REINHARDT_CLOUD_JWT_SECRET") };
		let cluster_id = Uuid::now_v7();

		// Act
		let result = issue_agent_token(cluster_id);

		// Cleanup — restore prior values so other serial tests are unaffected.
		match prior_dir {
			Some(v) => unsafe { std::env::set_var("REINHARDT_CLOUD_CONFIG_DIR", v) },
			None => unsafe { std::env::remove_var("REINHARDT_CLOUD_CONFIG_DIR") },
		}
		if let Some(v) = prior_secret {
			unsafe { std::env::set_var("REINHARDT_CLOUD_JWT_SECRET", v) };
		}

		// Assert
		assert!(result.is_err());
	}
}
