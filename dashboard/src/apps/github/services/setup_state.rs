//! Signed state values for GitHub App setup callbacks.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use hmac::{Hmac, Mac};
use rand::RngCore;
use sha2::Sha256;
use subtle::ConstantTimeEq;
use thiserror::Error;
use uuid::Uuid;

const SETUP_STATE_TTL_SECONDS: i64 = 600;
const SETUP_STATE_PURPOSE: &str = "github_setup_v1";

type HmacSha256 = Hmac<Sha256>;

/// Error returned when a GitHub setup state value is malformed or invalid.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum GitHubSetupStateError {
	#[error("GitHub setup state is malformed")]
	Malformed,
	#[error("GitHub setup state is expired")]
	Expired,
	#[error("GitHub setup state does not match the current user and organization")]
	ContextMismatch,
	#[error("GitHub setup state signature is invalid")]
	InvalidSignature,
}

/// Creates a signed, time-limited state parameter for GitHub App setup.
pub fn issue_setup_state(user_id: Uuid, organization_id: i64, secret: &str) -> String {
	let expires_at = chrono::Utc::now().timestamp() + SETUP_STATE_TTL_SECONDS;
	let mut nonce = [0_u8; 16];
	rand::rng().fill_bytes(&mut nonce);
	let nonce = URL_SAFE_NO_PAD.encode(nonce);
	let payload = format!("{SETUP_STATE_PURPOSE}|{user_id}|{organization_id}|{expires_at}|{nonce}");
	let payload_b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
	let signature = sign(secret, &payload_b64);
	let signature_b64 = URL_SAFE_NO_PAD.encode(signature);
	format!("{payload_b64}.{signature_b64}")
}

/// Verifies a setup state parameter against the current user and organization.
pub fn verify_setup_state(
	state: &str,
	user_id: Uuid,
	organization_id: i64,
	secret: &str,
) -> Result<(), GitHubSetupStateError> {
	let (payload_b64, signature_b64) = state
		.split_once('.')
		.ok_or(GitHubSetupStateError::Malformed)?;
	let provided_signature = URL_SAFE_NO_PAD
		.decode(signature_b64)
		.map_err(|_| GitHubSetupStateError::Malformed)?;
	let expected_signature = sign(secret, payload_b64);
	if provided_signature
		.as_slice()
		.ct_eq(expected_signature.as_slice())
		.unwrap_u8()
		!= 1
	{
		return Err(GitHubSetupStateError::InvalidSignature);
	}
	let payload_bytes = URL_SAFE_NO_PAD
		.decode(payload_b64)
		.map_err(|_| GitHubSetupStateError::Malformed)?;
	let payload = String::from_utf8(payload_bytes).map_err(|_| GitHubSetupStateError::Malformed)?;
	let parts = payload.split('|').collect::<Vec<_>>();
	if parts.len() != 5 || parts[0] != SETUP_STATE_PURPOSE {
		return Err(GitHubSetupStateError::Malformed);
	}
	let state_user_id = parts[1]
		.parse::<Uuid>()
		.map_err(|_| GitHubSetupStateError::Malformed)?;
	let state_organization_id = parts[2]
		.parse::<i64>()
		.map_err(|_| GitHubSetupStateError::Malformed)?;
	let expires_at = parts[3]
		.parse::<i64>()
		.map_err(|_| GitHubSetupStateError::Malformed)?;
	if state_user_id != user_id || state_organization_id != organization_id {
		return Err(GitHubSetupStateError::ContextMismatch);
	}
	if expires_at < chrono::Utc::now().timestamp() {
		return Err(GitHubSetupStateError::Expired);
	}
	Ok(())
}

/// Appends a setup state parameter to a configured GitHub App installation URL.
pub fn install_url_with_state(install_url: &str, state: &str) -> Result<String, url::ParseError> {
	let mut url = url::Url::parse(install_url)?;
	url.query_pairs_mut().append_pair("state", state);
	Ok(url.to_string())
}

fn sign(secret: &str, payload_b64: &str) -> Vec<u8> {
	let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
		.expect("HMAC accepts any key length for GitHub setup state signing");
	mac.update(payload_b64.as_bytes());
	mac.finalize().into_bytes().to_vec()
}
