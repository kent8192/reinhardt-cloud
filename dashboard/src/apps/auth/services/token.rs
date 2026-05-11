//! Stateless HMAC-based token generation and verification for auth flows.
//!
//! Tokens are signed with the application's `secret_key` using HMAC-SHA256.
//! No database table is needed — verification is purely computational.
//!
//! No `#[injectable_factory]` conversion (kent8192/reinhardt-cloud#599):
//! every function in this module is a pure cryptographic primitive that
//! takes its `secret_key` as an explicit parameter. There is no global
//! settings access and no environment-variable read, so the
//! ergonomic benefit of DI does not apply. Callers that need the
//! secret key resolve it from settings (or, in the future, from a
//! DI-resolved newtype) and pass it in.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// Token purpose discriminator — prevents cross-flow token misuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenPurpose {
	/// Email verification after registration.
	EmailVerification,
	/// Password reset via "forgot password" flow.
	PasswordReset,
}

impl TokenPurpose {
	pub(crate) fn as_str(self) -> &'static str {
		match self {
			Self::EmailVerification => "ev",
			Self::PasswordReset => "pr",
		}
	}

	fn from_str(s: &str) -> Option<Self> {
		match s {
			"ev" => Some(Self::EmailVerification),
			"pr" => Some(Self::PasswordReset),
			_ => None,
		}
	}
}

/// Errors returned by token verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
	/// Token has expired.
	Expired,
	/// Token signature is invalid (tampered or wrong key).
	InvalidSignature,
	/// Token format is malformed.
	MalformedToken,
	/// Token purpose does not match the expected purpose.
	PurposeMismatch,
	/// User not found for the embedded user ID.
	UserNotFound,
}

impl std::fmt::Display for TokenError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Expired => write!(f, "Token has expired"),
			Self::InvalidSignature => write!(f, "Invalid token"),
			Self::MalformedToken => write!(f, "Invalid token"),
			Self::PurposeMismatch => write!(f, "Invalid token"),
			Self::UserNotFound => write!(f, "Invalid token"),
		}
	}
}

/// Default expiry durations in seconds.
const EMAIL_VERIFICATION_EXPIRY_SECS: i64 = 86400; // 24 hours
const PASSWORD_RESET_EXPIRY_SECS: i64 = 3600; // 1 hour

/// Generate an HMAC-signed token for the given purpose.
///
/// The `password_hash` parameter is included in the HMAC input for
/// password-reset tokens so the token self-invalidates after a password
/// change. For email verification, pass an empty string.
///
/// Token format: `base64url(purpose|user_id|expiry_ts|hash_prefix).base64url(hmac_sig)`
pub fn generate_token(
	purpose: TokenPurpose,
	user_id: &Uuid,
	password_hash: &str,
	secret_key: &str,
) -> String {
	let expiry_secs = match purpose {
		TokenPurpose::EmailVerification => EMAIL_VERIFICATION_EXPIRY_SECS,
		TokenPurpose::PasswordReset => PASSWORD_RESET_EXPIRY_SECS,
	};
	let expiry_ts = chrono::Utc::now().timestamp() + expiry_secs;

	let hash_prefix = password_hash_prefix(password_hash);
	let payload = format!(
		"{}|{}|{}|{}",
		purpose.as_str(),
		user_id,
		expiry_ts,
		hash_prefix
	);

	let payload_b64 = base64url_encode(payload.as_bytes());
	let sig = compute_hmac(secret_key, &payload_b64);
	let sig_b64 = base64url_encode(&sig);

	format!("{payload_b64}.{sig_b64}")
}

/// Verify an HMAC-signed token and return the embedded user ID on success.
///
/// The `expected_purpose` ensures that email-verification tokens cannot be
/// used for password resets and vice versa.
///
/// The `password_hash` must match what was used when generating the token;
/// for password-reset tokens, a changed password will invalidate the token.
pub fn verify_token(
	token: &str,
	expected_purpose: TokenPurpose,
	password_hash: &str,
	secret_key: &str,
) -> Result<Uuid, TokenError> {
	let (payload_b64, sig_b64) = token.split_once('.').ok_or(TokenError::MalformedToken)?;

	// Verify HMAC signature in constant time
	let expected_sig = compute_hmac(secret_key, payload_b64);
	let provided_sig = base64url_decode(sig_b64).map_err(|_| TokenError::MalformedToken)?;

	if expected_sig.ct_eq(&provided_sig).unwrap_u8() != 1 {
		return Err(TokenError::InvalidSignature);
	}

	// Decode and parse payload
	let payload_bytes = base64url_decode(payload_b64).map_err(|_| TokenError::MalformedToken)?;
	let payload = String::from_utf8(payload_bytes).map_err(|_| TokenError::MalformedToken)?;

	let parts: Vec<&str> = payload.split('|').collect();
	if parts.len() != 4 {
		return Err(TokenError::MalformedToken);
	}

	let purpose = TokenPurpose::from_str(parts[0]).ok_or(TokenError::MalformedToken)?;
	if purpose != expected_purpose {
		return Err(TokenError::PurposeMismatch);
	}

	let user_id = parts[1]
		.parse::<Uuid>()
		.map_err(|_| TokenError::MalformedToken)?;

	let expiry_ts = parts[2]
		.parse::<i64>()
		.map_err(|_| TokenError::MalformedToken)?;

	if chrono::Utc::now().timestamp() > expiry_ts {
		return Err(TokenError::Expired);
	}

	// Verify password hash prefix matches (prevents token reuse after
	// password change for reset tokens)
	let expected_hash_prefix = password_hash_prefix(password_hash);
	if parts[3] != expected_hash_prefix {
		return Err(TokenError::InvalidSignature);
	}

	Ok(user_id)
}

/// Extract a short fingerprint from the password hash output.
///
/// Uses the last 16 characters of the hash string, which correspond to the
/// tail of the Argon2id hash output and are unique per password. The first 8
/// characters of an Argon2id hash are always `$argon2i` (the algorithm
/// identifier), so taking a prefix would fail to detect password changes.
fn password_hash_prefix(hash: &str) -> String {
	if hash.is_empty() {
		return String::new();
	}
	let chars: Vec<char> = hash.chars().collect();
	let start = chars.len().saturating_sub(16);
	chars[start..].iter().collect()
}

/// Compute HMAC-SHA256 for pre-DB validation in reset_password view.
///
/// This allows the view to verify the token signature before hitting
/// the database, rejecting tampered tokens cheaply.
pub(crate) fn compute_hmac_for_validation(secret_key: &str, data: &str) -> Vec<u8> {
	compute_hmac(secret_key, data)
}

fn compute_hmac(secret_key: &str, data: &str) -> Vec<u8> {
	let mut mac =
		HmacSha256::new_from_slice(secret_key.as_bytes()).expect("HMAC accepts any key length");
	mac.update(data.as_bytes());
	mac.finalize().into_bytes().to_vec()
}

fn base64url_encode(data: &[u8]) -> String {
	let standard = base64_encode_no_pad(data);
	let mut out = String::with_capacity(standard.len());
	for c in standard.chars() {
		match c {
			'+' => out.push('-'),
			'/' => out.push('_'),
			_ => out.push(c),
		}
	}
	out
}

pub(crate) fn base64url_decode(data: &str) -> Result<Vec<u8>, ()> {
	let mut standard = String::with_capacity(data.len());
	for c in data.chars() {
		match c {
			'-' => standard.push('+'),
			'_' => standard.push('/'),
			_ => standard.push(c),
		}
	}
	// Add padding
	while !standard.len().is_multiple_of(4) {
		standard.push('=');
	}
	base64_decode(&standard).map_err(|_| ())
}

// Minimal base64 encode/decode without external crate dependency.

fn base64_encode_no_pad(data: &[u8]) -> String {
	const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
	let mut out = String::new();
	let mut i = 0;
	while i + 2 < data.len() {
		let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8) | (data[i + 2] as u32);
		out.push(CHARS[(n >> 18 & 0x3F) as usize] as char);
		out.push(CHARS[(n >> 12 & 0x3F) as usize] as char);
		out.push(CHARS[(n >> 6 & 0x3F) as usize] as char);
		out.push(CHARS[(n & 0x3F) as usize] as char);
		i += 3;
	}
	let remaining = data.len() - i;
	if remaining == 2 {
		let n = ((data[i] as u32) << 16) | ((data[i + 1] as u32) << 8);
		out.push(CHARS[(n >> 18 & 0x3F) as usize] as char);
		out.push(CHARS[(n >> 12 & 0x3F) as usize] as char);
		out.push(CHARS[(n >> 6 & 0x3F) as usize] as char);
	} else if remaining == 1 {
		let n = (data[i] as u32) << 16;
		out.push(CHARS[(n >> 18 & 0x3F) as usize] as char);
		out.push(CHARS[(n >> 12 & 0x3F) as usize] as char);
	}
	out
}

fn base64_decode(data: &str) -> Result<Vec<u8>, &'static str> {
	const DECODE: [u8; 128] = {
		let mut table = [255u8; 128];
		let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
		let mut i = 0;
		while i < 64 {
			table[chars[i] as usize] = i as u8;
			i += 1;
		}
		table
	};

	let bytes = data.as_bytes();
	let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
	let mut buf = 0u32;
	let mut bits = 0u32;

	for &b in bytes {
		if b == b'=' {
			break;
		}
		if b >= 128 || DECODE[b as usize] == 255 {
			return Err("invalid base64 character");
		}
		buf = (buf << 6) | DECODE[b as usize] as u32;
		bits += 6;
		if bits >= 8 {
			bits -= 8;
			out.push((buf >> bits) as u8);
			buf &= (1 << bits) - 1;
		}
	}
	Ok(out)
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::*;

	const TEST_SECRET: &str = "test-secret-key-for-hmac-token-generation";

	#[rstest]
	fn test_email_verification_token_roundtrip() {
		// Arrange
		let user_id = Uuid::new_v4();

		// Act
		let token = generate_token(TokenPurpose::EmailVerification, &user_id, "", TEST_SECRET);
		let result = verify_token(&token, TokenPurpose::EmailVerification, "", TEST_SECRET);

		// Assert
		assert_eq!(result, Ok(user_id));
	}

	#[rstest]
	fn test_password_reset_token_roundtrip() {
		// Arrange
		let user_id = Uuid::new_v4();
		let password_hash = "$argon2id$v=19$some-hash-value";

		// Act
		let token = generate_token(
			TokenPurpose::PasswordReset,
			&user_id,
			password_hash,
			TEST_SECRET,
		);
		let result = verify_token(
			&token,
			TokenPurpose::PasswordReset,
			password_hash,
			TEST_SECRET,
		);

		// Assert
		assert_eq!(result, Ok(user_id));
	}

	#[rstest]
	fn test_wrong_purpose_returns_error() {
		// Arrange
		let user_id = Uuid::new_v4();
		let token = generate_token(TokenPurpose::EmailVerification, &user_id, "", TEST_SECRET);

		// Act
		let result = verify_token(&token, TokenPurpose::PasswordReset, "", TEST_SECRET);

		// Assert
		assert_eq!(result, Err(TokenError::PurposeMismatch));
	}

	#[rstest]
	fn test_tampered_token_returns_error() {
		// Arrange
		let user_id = Uuid::new_v4();
		let token = generate_token(TokenPurpose::EmailVerification, &user_id, "", TEST_SECRET);

		// Act — tamper with the payload portion
		let tampered = format!("X{}", &token[1..]);
		let result = verify_token(&tampered, TokenPurpose::EmailVerification, "", TEST_SECRET);

		// Assert
		assert_eq!(result, Err(TokenError::InvalidSignature));
	}

	#[rstest]
	fn test_wrong_secret_key_returns_error() {
		// Arrange
		let user_id = Uuid::new_v4();
		let token = generate_token(TokenPurpose::EmailVerification, &user_id, "", TEST_SECRET);

		// Act
		let result = verify_token(
			&token,
			TokenPurpose::EmailVerification,
			"",
			"wrong-secret-key",
		);

		// Assert
		assert_eq!(result, Err(TokenError::InvalidSignature));
	}

	#[rstest]
	fn test_password_change_invalidates_reset_token() {
		// Arrange
		let user_id = Uuid::new_v4();
		let old_hash = "$argon2id$v=19$m=19456,t=2,p=1$oldsaltvalue$oldoutputhashABCD";
		let new_hash = "$argon2id$v=19$m=19456,t=2,p=1$newsaltvalue$newoutputhashEFGH";
		let token = generate_token(TokenPurpose::PasswordReset, &user_id, old_hash, TEST_SECRET);

		// Act — verify with the new password hash
		let result = verify_token(&token, TokenPurpose::PasswordReset, new_hash, TEST_SECRET);

		// Assert
		assert_eq!(result, Err(TokenError::InvalidSignature));
	}

	#[rstest]
	fn test_malformed_token_returns_error() {
		// Arrange / Act
		let result = verify_token(
			"not-a-valid-token",
			TokenPurpose::EmailVerification,
			"",
			TEST_SECRET,
		);

		// Assert — either MalformedToken or InvalidSignature
		assert!(result.is_err());
	}

	#[rstest]
	fn test_base64url_roundtrip() {
		// Arrange
		let data = b"hello world! special chars: +/=";

		// Act
		let encoded = base64url_encode(data);
		let decoded = base64url_decode(&encoded).expect("decode should succeed");

		// Assert
		assert_eq!(decoded, data);
	}
}
