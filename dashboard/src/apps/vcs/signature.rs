//! Webhook signature verification for GitHub and GitLab.

use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Verifies a GitHub webhook signature (X-Hub-Signature-256).
pub fn verify_github_signature(secret: &[u8], payload: &[u8], signature: &str) -> bool {
	let expected_prefix = "sha256=";
	let hex_digest = match signature.strip_prefix(expected_prefix) {
		Some(h) => h,
		None => return false,
	};
	let expected_bytes = match hex::decode(hex_digest) {
		Ok(b) => b,
		Err(_) => return false,
	};
	let mut mac = match HmacSha256::new_from_slice(secret) {
		Ok(m) => m,
		Err(_) => return false,
	};
	mac.update(payload);
	mac.verify_slice(&expected_bytes).is_ok()
}

/// Verifies a GitLab webhook token (X-Gitlab-Token).
pub fn verify_gitlab_token(secret: &str, token: &str) -> bool {
	use subtle::ConstantTimeEq;
	secret.as_bytes().ct_eq(token.as_bytes()).into()
}

#[cfg(test)]
mod tests {
	use super::*;
	use hmac::Mac;

	fn compute_github_signature(secret: &[u8], payload: &[u8]) -> String {
		let mut mac = HmacSha256::new_from_slice(secret).unwrap();
		mac.update(payload);
		let result = mac.finalize().into_bytes();
		format!("sha256={}", hex::encode(result))
	}

	#[test]
	fn test_verify_github_signature_valid() {
		let secret = b"my-secret-key";
		let payload = b"hello world";
		let signature = compute_github_signature(secret, payload);
		assert!(verify_github_signature(secret, payload, &signature));
	}

	#[test]
	fn test_verify_github_signature_invalid() {
		let secret = b"my-secret-key";
		let payload = b"hello world";
		let signature = compute_github_signature(secret, payload);
		// Tamper with payload
		assert!(!verify_github_signature(secret, b"tampered", &signature));
	}

	#[test]
	fn test_verify_github_signature_wrong_prefix() {
		let secret = b"my-secret-key";
		let payload = b"hello world";
		assert!(!verify_github_signature(secret, payload, "md5=abc123"));
	}

	#[test]
	fn test_verify_github_signature_invalid_hex() {
		let secret = b"my-secret-key";
		let payload = b"hello world";
		assert!(!verify_github_signature(secret, payload, "sha256=not-hex!"));
	}

	#[test]
	fn test_verify_gitlab_token_valid() {
		assert!(verify_gitlab_token("my-token", "my-token"));
	}

	#[test]
	fn test_verify_gitlab_token_invalid() {
		assert!(!verify_gitlab_token("my-token", "wrong-token"));
	}

	#[test]
	fn test_verify_gitlab_token_empty() {
		assert!(!verify_gitlab_token("my-token", ""));
	}
}
