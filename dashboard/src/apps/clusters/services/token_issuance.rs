//! Cluster agent JWT token issuance service.
//!
//! Provides [`AgentTokenService`] (resolved via `#[injectable_factory]`)
//! that mints agent JWTs and produces Argon2id hashes for persistence.
//! The plaintext is returned exactly once to the caller and never
//! persisted.
//!
//! The legacy free functions [`jwt_secret`] and [`issue_agent_token`]
//! are retained as thin adapters during the kent8192/reinhardt-cloud#599
//! caller migration and will be removed once all callers resolve
//! [`AgentTokenService`] via DI.

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

/// Load the JWT signing secret.
///
/// Resolves via [`crate::config::settings::get_jwt_secret`], which
/// reads the top-level `jwt_secret` key from the active TOML profile
/// and falls back to the `REINHARDT_CLOUD_JWT_SECRET` environment
/// variable. Returns an error only when neither source supplies a
/// value.
///
/// Retained as a thin adapter while callers migrate to resolving
/// [`JwtSecret`] via DI (kent8192/reinhardt-cloud#599). Will be
/// removed once `dashboard/src/config/grpc.rs` consumes the secret
/// through `Depends<JwtSecret>`.
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
///
/// Retained as a thin adapter while callers migrate to resolving
/// [`AgentTokenService`] via DI (kent8192/reinhardt-cloud#599). Will be
/// removed once cluster view handlers resolve the service through
/// `Depends<AgentTokenService>`.
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
	use crate::config::test_helpers::make_test_di_context;
	use rstest::rstest;
	use serial_test::serial;
	use std::sync::Arc;

	/// Path that is guaranteed not to contain settings TOML files.
	/// Used to neutralise TOML lookup so env-var resolution can be tested
	/// in isolation (`get_jwt_secret` falls back when TOML is absent).
	const NONEXISTENT_CONFIG_DIR: &str = "/nonexistent-test-config-dir-494";

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

	// Legacy free-function tests — retained until commit 9 deletes the
	// `pub fn issue_agent_token` adapter alongside these tests.

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
