//! Encryption helpers for persisted OAuth access tokens.

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use thiserror::Error;

const TOKEN_ENCRYPTION_KEY_ENV: &str = "REINHARDT_CLOUD_OAUTH_TOKEN_ENCRYPTION_KEY";
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

/// Errors returned by OAuth token encryption/decryption.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum OAuthTokenCryptoError {
	#[error("{TOKEN_ENCRYPTION_KEY_ENV} is required")]
	MissingKey,
	#[error("{TOKEN_ENCRYPTION_KEY_ENV} must be base64-encoded 32 bytes")]
	InvalidKey,
	#[error("encrypted OAuth token payload is invalid")]
	InvalidPayload,
	#[error("failed to encrypt OAuth token")]
	Encrypt,
	#[error("failed to decrypt OAuth token")]
	Decrypt,
	#[error("OAuth token is not valid UTF-8")]
	Utf8,
}

/// Encrypt an OAuth access token with the configured process key.
pub fn encrypt_access_token(access_token: &str) -> Result<String, OAuthTokenCryptoError> {
	let key = key_from_env()?;
	encrypt_access_token_with_key(access_token, &key)
}

/// Decrypt an OAuth access token with the configured process key.
pub fn decrypt_access_token(encrypted: &str) -> Result<String, OAuthTokenCryptoError> {
	let key = key_from_env()?;
	decrypt_access_token_with_key(encrypted, &key)
}

/// Return whether the process has a valid OAuth token encryption key.
pub(crate) fn token_encryption_key_is_configured() -> bool {
	key_from_env().is_ok()
}

pub(crate) fn encrypt_access_token_with_key(
	access_token: &str,
	key_bytes: &[u8; KEY_LEN],
) -> Result<String, OAuthTokenCryptoError> {
	let cipher = cipher_from_key(key_bytes);
	let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
	let ciphertext = cipher
		.encrypt(&nonce, access_token.as_bytes())
		.map_err(|_| OAuthTokenCryptoError::Encrypt)?;
	let mut payload = Vec::with_capacity(NONCE_LEN + ciphertext.len());
	payload.extend_from_slice(&nonce);
	payload.extend_from_slice(&ciphertext);
	Ok(STANDARD.encode(payload))
}

pub(crate) fn decrypt_access_token_with_key(
	encrypted: &str,
	key_bytes: &[u8; KEY_LEN],
) -> Result<String, OAuthTokenCryptoError> {
	let payload = STANDARD
		.decode(encrypted)
		.map_err(|_| OAuthTokenCryptoError::InvalidPayload)?;
	if payload.len() <= NONCE_LEN {
		return Err(OAuthTokenCryptoError::InvalidPayload);
	}
	let (nonce, ciphertext) = payload.split_at(NONCE_LEN);
	let cipher = cipher_from_key(key_bytes);
	let plaintext = cipher
		.decrypt(Nonce::from_slice(nonce), ciphertext)
		.map_err(|_| OAuthTokenCryptoError::Decrypt)?;
	String::from_utf8(plaintext).map_err(|_| OAuthTokenCryptoError::Utf8)
}

fn key_from_env() -> Result<[u8; KEY_LEN], OAuthTokenCryptoError> {
	let raw =
		std::env::var(TOKEN_ENCRYPTION_KEY_ENV).map_err(|_| OAuthTokenCryptoError::MissingKey)?;
	let decoded = STANDARD
		.decode(raw)
		.map_err(|_| OAuthTokenCryptoError::InvalidKey)?;
	decoded
		.try_into()
		.map_err(|_| OAuthTokenCryptoError::InvalidKey)
}

fn cipher_from_key(key_bytes: &[u8; KEY_LEN]) -> Aes256Gcm {
	Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key_bytes))
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_access_token_encryption_roundtrip() {
		// Arrange
		let key = [7u8; KEY_LEN];

		// Act
		let encrypted =
			encrypt_access_token_with_key("ghu_secret", &key).expect("encrypt should succeed");
		let decrypted =
			decrypt_access_token_with_key(&encrypted, &key).expect("decrypt should succeed");

		// Assert
		assert_eq!(decrypted, "ghu_secret");
		assert_ne!(encrypted, "ghu_secret");
	}

	#[rstest]
	fn test_access_token_decryption_rejects_wrong_key() {
		// Arrange
		let encrypted = encrypt_access_token_with_key("ghu_secret", &[1u8; KEY_LEN])
			.expect("encrypt should succeed");

		// Act
		let result = decrypt_access_token_with_key(&encrypted, &[2u8; KEY_LEN]);

		// Assert
		assert_eq!(result, Err(OAuthTokenCryptoError::Decrypt));
	}
}
