//! Cluster agent JWT token issuance service.
//!
//! Provides [`AgentTokenService`] (resolved via `#[injectable_factory]`)
//! that mints agent JWTs and produces Argon2id hashes for persistence.
//! The plaintext is returned exactly once to the caller and never
//! persisted.

use reinhardt::Argon2Hasher;
use reinhardt::PasswordHasher;
use reinhardt::core::exception::Error as AppError;
use reinhardt::di::{Depends, injectable_factory};
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

/// JWT signing secret captured at DI resolution time.
///
/// Wrapper newtype to satisfy the DI pseudo-orphan rule
/// (kent8192/reinhardt-web#3468) — `String` cannot be registered
/// directly. Singleton-scoped so the secret is read once at first
/// resolve and shared across all subsequent service instantiations.
pub struct JwtSecret(pub String);

/// DI factory — resolves the JWT secret from the active settings
/// profile (TOML key `jwt_secret` with the
/// `REINHARDT_CLOUD_JWT_SECRET` env-var fallback). Panics if no source
/// supplies a value, which is treated as a deploy-time configuration
/// error rather than a recoverable runtime fault.
#[injectable_factory(scope = "singleton")]
async fn create_jwt_secret() -> JwtSecret {
	JwtSecret(crate::config::settings::get_jwt_secret().expect(
		"JWT secret not configured: set jwt_secret in TOML or REINHARDT_CLOUD_JWT_SECRET env var",
	))
}

/// Cluster agent token issuance service.
///
/// Holds the JWT secret captured at factory time so individual `issue`
/// calls do not re-read settings or hold global locks.
pub struct AgentTokenService {
	jwt_secret: String,
}

/// DI factory — `transient` because the service is cheap to clone and
/// may be resolved per request without contention.
#[injectable_factory(scope = "transient")]
async fn create_agent_token_service(
	#[inject] jwt_secret: Depends<JwtSecret>,
) -> AgentTokenService {
	AgentTokenService {
		jwt_secret: jwt_secret.0.clone(),
	}
}

impl AgentTokenService {
	/// Mint a cluster agent JWT and return both the plaintext and its
	/// Argon2id hash.
	pub fn issue(&self, cluster_id: Uuid) -> Result<IssuedAgentToken, AppError> {
		let plaintext = create_agent_token(
			cluster_id,
			self.jwt_secret.as_bytes(),
			AGENT_TOKEN_EXPIRY_HOURS,
		)
		.map_err(|e| AppError::Internal(format!("Failed to mint agent token: {e}")))?;

		let hasher = Argon2Hasher::new();
		let hash = hasher
			.hash(&plaintext)
			.map_err(|e| AppError::Internal(format!("Failed to hash agent token: {e}")))?;

		Ok(IssuedAgentToken { plaintext, hash })
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::config::test_helpers::make_test_di_context;
	use rstest::rstest;
	use std::sync::Arc;

	#[rstest]
	#[tokio::test]
	async fn test_agent_token_service_factory_resolves_with_overridden_secret() {
		// Arrange — override JwtSecret so the factory does not touch
		// global settings; tests run in parallel without serial_test locks.
		let ctx = make_test_di_context(|scope| {
			scope.set(JwtSecret("test-secret-do-not-use-in-prod".into()));
		});

		// Act
		let svc: Arc<AgentTokenService> = ctx
			.resolve::<AgentTokenService>()
			.await
			.expect("AgentTokenService factory should resolve when JwtSecret is registered");
		let cluster_id = Uuid::now_v7();
		let issued = svc.issue(cluster_id).expect("issue should succeed");

		// Assert
		assert!(!issued.plaintext.is_empty());
		assert!(!issued.hash.is_empty());
		assert_ne!(issued.plaintext, issued.hash);

		let hasher = Argon2Hasher::new();
		let verified = hasher
			.verify(&issued.plaintext, &issued.hash)
			.expect("verify should not error");
		assert!(verified);
	}

	#[rstest]
	#[tokio::test]
	async fn test_agent_token_service_issued_token_contains_cluster_id_claim() {
		// Arrange
		let ctx = make_test_di_context(|scope| {
			scope.set(JwtSecret("test-secret-claim-check".into()));
		});
		let svc: Arc<AgentTokenService> = ctx
			.resolve::<AgentTokenService>()
			.await
			.expect("factory should resolve");
		let cluster_id = Uuid::now_v7();

		// Act
		let issued = svc.issue(cluster_id).expect("issue should succeed");
		let claims = reinhardt_cloud_grpc::agent_claims::verify_agent_token(
			&issued.plaintext,
			b"test-secret-claim-check",
		)
		.expect("token should verify with the secret used to mint it");

		// Assert
		assert_eq!(claims.cluster_id, cluster_id.to_string());
	}
}
