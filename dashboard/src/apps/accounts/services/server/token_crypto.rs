//! Encryption of provider tokens at rest (SR-06).
//!
//! Tokens are sealed with AES-256-GCM. Each ciphertext is stored as the
//! envelope `v1.<key id>.<base64url(nonce || ciphertext || tag)>`, so a later
//! key rotation can tell which key sealed which value without a data
//! migration. The associated data binds a ciphertext to the User it belongs to
//! and to its purpose (access or refresh token): a value copied to another
//! row or column fails authentication instead of decrypting.
//!
//! The key never touches the database. It comes from settings, which read it
//! from the environment, or is derived from `core.secret_key` when no
//! dedicated key is configured. A malformed key fails keyring construction, so
//! sign-in cannot start without a usable key.

use std::collections::HashSet;
use std::fmt;

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use hkdf::Hkdf;
use reinhardt::conf::settings::secret_types::SecretString;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const ENVELOPE_VERSION: &str = "v1";
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const MAX_KEY_ID_LEN: usize = 32;
const DEFAULT_KEY_ID: &str = "primary";
const DERIVATION_INFO: &[u8] = b"reinhardt-cloud/accounts/provider-token-key/v1";
const AAD_DOMAIN: &[u8] = b"reinhardt-cloud/accounts/provider-token/v1";

/// Which token a ciphertext holds. Part of the associated data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenPurpose {
	/// A GitHub user access token.
	Access,
	/// A GitHub user refresh token.
	Refresh,
}

impl TokenPurpose {
	const fn label(self) -> &'static [u8] {
		match self {
			Self::Access => b"access",
			Self::Refresh => b"refresh",
		}
	}
}

/// What a ciphertext is bound to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TokenContext {
	/// The User that owns the token.
	pub user_id: Uuid,
	/// Which token it is.
	pub purpose: TokenPurpose,
}

impl TokenContext {
	fn associated_data(self) -> Vec<u8> {
		let mut aad = Vec::with_capacity(AAD_DOMAIN.len() + 2 + 16 + 8);
		aad.extend_from_slice(AAD_DOMAIN);
		aad.push(0);
		aad.extend_from_slice(self.user_id.as_bytes());
		aad.push(0);
		aad.extend_from_slice(self.purpose.label());
		aad
	}
}

/// Errors from keyring construction and token sealing. None carries key
/// material or token text.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TokenCryptoError {
	/// `core.secret_key` is empty and no dedicated key is configured.
	#[error("no key material: configure the token encryption key or core.secret_key")]
	MissingKeyMaterial,
	/// The configured key is not 32 bytes of standard base64.
	#[error("token encryption key must be base64 of exactly 32 bytes")]
	MalformedKey,
	/// A key identifier is empty, too long, or uses characters outside `[A-Za-z0-9_-]`.
	#[error("token encryption key id must be 1 to 32 characters of [A-Za-z0-9_-]")]
	MalformedKeyId,
	/// A key identifier was configured without a key to attach it to.
	#[error("token encryption key id is set but no token encryption key is configured")]
	KeyIdWithoutKey,
	/// An entry of the retired key list is not `id:base64`.
	#[error("retired token encryption keys must be comma-separated id:base64 pairs")]
	MalformedRetiredKeys,
	/// Two keys share an identifier.
	#[error("token encryption key ids must be unique")]
	DuplicateKeyId,
	/// A stored value is not a well-formed envelope.
	#[error("stored token is not a valid encrypted envelope")]
	MalformedEnvelope,
	/// The envelope names a key the keyring does not hold.
	#[error("stored token was sealed with unknown key {key_id:?}")]
	UnknownKey {
		/// The key identifier read from the envelope.
		key_id: String,
	},
	/// Authentication failed: wrong key, wrong owner or purpose, or tampering.
	#[error("stored token could not be decrypted")]
	Decrypt,
	/// Sealing failed.
	#[error("token could not be encrypted")]
	Encrypt,
}

/// Raw key settings, borrowed from `AccountsSettings`.
#[derive(Clone, Copy)]
pub struct TokenKeyringSettings<'a> {
	/// Dedicated key (base64 of 32 bytes), when configured.
	pub key: Option<&'a SecretString>,
	/// Identifier of the dedicated key; empty selects the default.
	pub key_id: &'a str,
	/// Comma-separated `id:base64` pairs of retired keys.
	pub retired_keys: Option<&'a SecretString>,
}

struct TokenKey {
	id: String,
	cipher: Aes256Gcm,
}

impl TokenKey {
	fn from_bytes(id: String, bytes: &[u8; KEY_LEN]) -> Self {
		Self {
			id,
			cipher: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(bytes)),
		}
	}
}

/// The active token key plus retired keys kept for decryption.
pub struct TokenKeyring {
	active: TokenKey,
	retired: Vec<TokenKey>,
}

// Manual impl: the derived one would walk into the cipher state.
impl fmt::Debug for TokenKeyring {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("TokenKeyring")
			.field("active_key_id", &self.active.id)
			.field(
				"retired_key_ids",
				&self.retired.iter().map(|key| &key.id).collect::<Vec<_>>(),
			)
			.finish_non_exhaustive()
	}
}

impl TokenKeyring {
	/// Build the keyring from settings.
	///
	/// With a dedicated key, that key is active. Without one, the active key is
	/// derived from `secret_key` (HKDF-SHA256 with a fixed domain label), so the
	/// session secret itself is never used directly as an encryption key.
	///
	/// # Errors
	///
	/// Returns an error for a malformed key, key id, or retired-key list, or
	/// when no key material is available.
	pub fn from_settings(
		settings: TokenKeyringSettings<'_>,
		secret_key: &str,
	) -> Result<Self, TokenCryptoError> {
		let dedicated = settings.key.filter(|key| !key.is_empty());
		let key_id = settings.key_id.trim();
		let active = match dedicated {
			Some(key) => {
				let id = if key_id.is_empty() {
					DEFAULT_KEY_ID
				} else {
					key_id
				};
				validate_key_id(id)?;
				TokenKey::from_bytes(id.to_owned(), &decode_key(key.expose_secret())?)
			}
			None => {
				if !key_id.is_empty() {
					return Err(TokenCryptoError::KeyIdWithoutKey);
				}
				derive_key(secret_key)?
			}
		};

		let mut retired = Vec::new();
		if let Some(list) = settings.retired_keys {
			for entry in list
				.expose_secret()
				.split(',')
				.map(str::trim)
				.filter(|entry| !entry.is_empty())
			{
				let (id, key) = entry
					.split_once(':')
					.ok_or(TokenCryptoError::MalformedRetiredKeys)?;
				validate_key_id(id)?;
				retired.push(TokenKey::from_bytes(id.to_owned(), &decode_key(key)?));
			}
		}

		let mut seen = HashSet::new();
		if !std::iter::once(&active)
			.chain(&retired)
			.all(|key| seen.insert(key.id.as_str()))
		{
			return Err(TokenCryptoError::DuplicateKeyId);
		}
		Ok(Self { active, retired })
	}

	/// Identifier of the key new tokens are sealed with.
	#[must_use]
	pub fn active_key_id(&self) -> &str {
		&self.active.id
	}

	/// Seal `plaintext` into an envelope bound to `context`.
	///
	/// # Errors
	///
	/// Returns [`TokenCryptoError::Encrypt`] if sealing fails.
	pub fn encrypt(
		&self,
		plaintext: &SecretString,
		context: TokenContext,
	) -> Result<String, TokenCryptoError> {
		let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
		let aad = context.associated_data();
		let sealed = self
			.active
			.cipher
			.encrypt(
				&nonce,
				Payload {
					msg: plaintext.expose_secret().as_bytes(),
					aad: &aad,
				},
			)
			.map_err(|_| TokenCryptoError::Encrypt)?;
		let mut body = Vec::with_capacity(NONCE_LEN + sealed.len());
		body.extend_from_slice(&nonce);
		body.extend_from_slice(&sealed);
		Ok(format!(
			"{ENVELOPE_VERSION}.{}.{}",
			self.active.id,
			URL_SAFE_NO_PAD.encode(body)
		))
	}

	/// Open an envelope. A wrong key, a different owner or purpose, and any
	/// modification all fail; nothing is returned in place of the plaintext.
	///
	/// # Errors
	///
	/// Returns an error when the envelope is malformed, names an unknown key,
	/// or fails authentication.
	pub fn decrypt(
		&self,
		envelope: &str,
		context: TokenContext,
	) -> Result<SecretString, TokenCryptoError> {
		let mut parts = envelope.split('.');
		let (Some(version), Some(key_id), Some(body), None) =
			(parts.next(), parts.next(), parts.next(), parts.next())
		else {
			return Err(TokenCryptoError::MalformedEnvelope);
		};
		if version != ENVELOPE_VERSION {
			return Err(TokenCryptoError::MalformedEnvelope);
		}
		let key = std::iter::once(&self.active)
			.chain(&self.retired)
			.find(|key| key.id == key_id)
			.ok_or_else(|| TokenCryptoError::UnknownKey {
				key_id: key_id.to_owned(),
			})?;
		let body = URL_SAFE_NO_PAD
			.decode(body)
			.map_err(|_| TokenCryptoError::MalformedEnvelope)?;
		if body.len() <= NONCE_LEN {
			return Err(TokenCryptoError::MalformedEnvelope);
		}
		let (nonce, sealed) = body.split_at(NONCE_LEN);
		let aad = context.associated_data();
		let plaintext = key
			.cipher
			.decrypt(
				Nonce::from_slice(nonce),
				Payload {
					msg: sealed,
					aad: &aad,
				},
			)
			.map_err(|_| TokenCryptoError::Decrypt)?;
		String::from_utf8(plaintext)
			.map(SecretString::new)
			.map_err(|_| TokenCryptoError::Decrypt)
	}
}

fn decode_key(encoded: &str) -> Result<[u8; KEY_LEN], TokenCryptoError> {
	STANDARD
		.decode(encoded.trim())
		.ok()
		.and_then(|bytes| <[u8; KEY_LEN]>::try_from(bytes).ok())
		.ok_or(TokenCryptoError::MalformedKey)
}

fn validate_key_id(id: &str) -> Result<(), TokenCryptoError> {
	let valid = !id.is_empty()
		&& id.len() <= MAX_KEY_ID_LEN
		&& id
			.bytes()
			.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
	if valid {
		Ok(())
	} else {
		Err(TokenCryptoError::MalformedKeyId)
	}
}

/// Derive the fallback key from the application secret key.
fn derive_key(secret_key: &str) -> Result<TokenKey, TokenCryptoError> {
	if secret_key.is_empty() {
		return Err(TokenCryptoError::MissingKeyMaterial);
	}
	let mut bytes = [0u8; KEY_LEN];
	Hkdf::<Sha256>::new(None, secret_key.as_bytes())
		.expand(DERIVATION_INFO, &mut bytes)
		.map_err(|_| TokenCryptoError::MissingKeyMaterial)?;
	// The id is a fingerprint of the derived key: rotating `core.secret_key`
	// changes it, so old values report an unknown key instead of a bare
	// authentication failure.
	let fingerprint = Sha256::digest(bytes);
	let id = format!(
		"core-{:02x}{:02x}{:02x}{:02x}",
		fingerprint[0], fingerprint[1], fingerprint[2], fingerprint[3]
	);
	Ok(TokenKey::from_bytes(id, &bytes))
}
