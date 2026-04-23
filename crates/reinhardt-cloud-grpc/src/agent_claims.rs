//! JWT claims type for cluster agent authentication.
//!
//! Unlike the human user `Claims` in `reinhardt-cloud-core`, `AgentClaims`
//! carries a `cluster_id` that identifies which cluster an agent belongs
//! to. The gRPC `AgentJwtInterceptor` rejects tokens that are missing a
//! `cluster_id`, which is how an agent token is distinguished from a
//! user token.

use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JWT claims issued to a cluster agent.
///
/// The presence of `cluster_id` is what authorizes an agent to open a
/// bidirectional `AgentService/AgentStream` against the control plane.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct AgentClaims {
	/// Subject — the cluster agent's UUID (`cluster_id` or agent identity).
	pub sub: String,
	/// The cluster ID this token authorizes. Used by the registry to route
	/// agent commands to the correct open stream.
	pub cluster_id: String,
	/// Expiration time (Unix timestamp).
	pub exp: i64,
	/// Issued at (Unix timestamp).
	pub iat: i64,
	/// Optional issuer claim (defaults empty for compatibility).
	#[serde(default, skip_serializing_if = "String::is_empty")]
	pub iss: String,
	/// Optional audience claim (defaults empty for compatibility).
	#[serde(default, skip_serializing_if = "String::is_empty")]
	pub aud: String,
}

impl AgentClaims {
	/// Construct a new set of agent claims with sensible defaults.
	pub fn new(cluster_id: Uuid, expiry_hours: i64) -> Self {
		let now = Utc::now();
		Self {
			sub: cluster_id.to_string(),
			cluster_id: cluster_id.to_string(),
			exp: (now + Duration::hours(expiry_hours)).timestamp(),
			iat: now.timestamp(),
			iss: String::new(),
			aud: String::new(),
		}
	}
}

/// Create and sign an agent JWT token.
pub fn create_agent_token(
	cluster_id: Uuid,
	secret: &[u8],
	expiry_hours: i64,
) -> Result<String, jsonwebtoken::errors::Error> {
	let claims = AgentClaims::new(cluster_id, expiry_hours);
	encode(
		&Header::default(),
		&claims,
		&EncodingKey::from_secret(secret),
	)
}

/// Verify and decode an agent JWT token.
///
/// Returns `Err` when the signature is invalid, the token is expired,
/// or `cluster_id` is missing/empty.
pub fn verify_agent_token(
	token: &str,
	secret: &[u8],
) -> Result<AgentClaims, jsonwebtoken::errors::Error> {
	let data = decode::<AgentClaims>(
		token,
		&DecodingKey::from_secret(secret),
		&Validation::default(),
	)?;
	if data.claims.cluster_id.trim().is_empty() {
		// Reject agent tokens without a cluster_id — this is the signal
		// we use to distinguish agent tokens from user tokens.
		return Err(jsonwebtoken::errors::Error::from(
			jsonwebtoken::errors::ErrorKind::InvalidToken,
		));
	}
	Ok(data.claims)
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	const TEST_SECRET: &[u8] = b"test-secret-key-for-agent-jwt-signing";

	#[rstest]
	fn test_create_and_verify_agent_token_roundtrip() {
		// Arrange
		let cluster_id = Uuid::now_v7();

		// Act
		let token = create_agent_token(cluster_id, TEST_SECRET, 24).unwrap();
		let claims = verify_agent_token(&token, TEST_SECRET).unwrap();

		// Assert
		assert_eq!(claims.sub, cluster_id.to_string());
		assert_eq!(claims.cluster_id, cluster_id.to_string());
		assert!(claims.exp > claims.iat);
	}

	#[rstest]
	fn test_verify_agent_token_with_wrong_secret() {
		// Arrange
		let cluster_id = Uuid::now_v7();
		let token = create_agent_token(cluster_id, TEST_SECRET, 24).unwrap();

		// Act
		let result = verify_agent_token(&token, b"wrong-secret");

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_verify_expired_agent_token_is_rejected() {
		// Arrange
		let cluster_id = Uuid::now_v7();
		let token = create_agent_token(cluster_id, TEST_SECRET, -1).unwrap();

		// Act
		let result = verify_agent_token(&token, TEST_SECRET);

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_verify_agent_token_missing_cluster_id_is_rejected() {
		// Arrange — manually build a token that has no cluster_id
		let now = Utc::now();
		let claims_without_cluster = serde_json::json!({
			"sub": Uuid::now_v7().to_string(),
			"cluster_id": "",
			"exp": (now + Duration::hours(1)).timestamp(),
			"iat": now.timestamp(),
		});
		let token = encode(
			&Header::default(),
			&claims_without_cluster,
			&EncodingKey::from_secret(TEST_SECRET),
		)
		.unwrap();

		// Act
		let result = verify_agent_token(&token, TEST_SECRET);

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_agent_claims_serde_default_fields() {
		// Arrange
		let now = Utc::now().timestamp();
		let json = serde_json::json!({
			"sub": "agent-1",
			"cluster_id": "cluster-42",
			"exp": now + 3600,
			"iat": now,
		});

		// Act
		let claims: AgentClaims = serde_json::from_value(json).unwrap();

		// Assert — iss/aud default to empty when missing
		assert_eq!(claims.iss, "");
		assert_eq!(claims.aud, "");
		assert_eq!(claims.cluster_id, "cluster-42");
	}
}
