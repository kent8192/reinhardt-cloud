//! Unit tests of provider token encryption (SR-06, SR-102).

use rstest::{fixture, rstest};
use uuid::Uuid;

use reinhardt::conf::settings::secret_types::SecretString;

use crate::apps::accounts::services::server::token_crypto::{
	TokenContext, TokenCryptoError, TokenKeyring, TokenKeyringSettings, TokenPurpose,
};

const SECRET_KEY: &str = "test-core-secret-key-with-enough-entropy";
// Base64 of 32 bytes (all 0x07).
const KEY_A: &str = "BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc=";
// Base64 of 32 bytes (all 0x09).
const KEY_B: &str = "CQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQk=";

fn keyring_with(
	key: Option<&str>,
	key_id: &str,
	retired: Option<&str>,
) -> Result<TokenKeyring, TokenCryptoError> {
	let key = key.map(SecretString::new);
	let retired = retired.map(SecretString::new);
	TokenKeyring::from_settings(
		TokenKeyringSettings {
			key: key.as_ref(),
			key_id,
			retired_keys: retired.as_ref(),
		},
		SECRET_KEY,
	)
}

#[fixture]
fn context() -> TokenContext {
	TokenContext {
		user_id: Uuid::now_v7(),
		purpose: TokenPurpose::Access,
	}
}

#[rstest]
fn sr_06_token_round_trips_and_ciphertext_hides_plaintext(context: TokenContext) {
	// Arrange
	let keyring = keyring_with(Some(KEY_A), "", None).unwrap();
	let token = SecretString::new("ghu_plaintext_access_token");

	// Act
	let envelope = keyring.encrypt(&token, context).unwrap();
	let opened = keyring.decrypt(&envelope, context).unwrap();

	// Assert
	assert_eq!(opened.expose_secret(), "ghu_plaintext_access_token");
	assert!(envelope.starts_with("v1.primary."));
	assert!(!envelope.contains("ghu_plaintext_access_token"));
}

#[rstest]
fn sr_06_encrypting_twice_gives_different_ciphertexts(context: TokenContext) {
	// Arrange
	let keyring = keyring_with(Some(KEY_A), "", None).unwrap();
	let token = SecretString::new("ghu_same");

	// Act
	let first = keyring.encrypt(&token, context).unwrap();
	let second = keyring.encrypt(&token, context).unwrap();

	// Assert
	assert_ne!(first, second);
}

#[rstest]
fn sr_06_wrong_key_fails_decryption_instead_of_returning_garbage(context: TokenContext) {
	// Arrange
	let sealing = keyring_with(Some(KEY_A), "shared", None).unwrap();
	let opening = keyring_with(Some(KEY_B), "shared", None).unwrap();
	let envelope = sealing
		.encrypt(&SecretString::new("ghu_secret"), context)
		.unwrap();

	// Act
	let result = opening.decrypt(&envelope, context);

	// Assert
	assert_eq!(result.unwrap_err(), TokenCryptoError::Decrypt);
}

#[rstest]
fn sr_06_ciphertext_is_bound_to_its_owner_and_purpose(context: TokenContext) {
	// Arrange
	let keyring = keyring_with(Some(KEY_A), "", None).unwrap();
	let envelope = keyring
		.encrypt(&SecretString::new("ghu_secret"), context)
		.unwrap();
	let other_user = TokenContext {
		user_id: Uuid::now_v7(),
		..context
	};
	let other_purpose = TokenContext {
		purpose: TokenPurpose::Refresh,
		..context
	};

	// Act
	let as_other_user = keyring.decrypt(&envelope, other_user);
	let as_other_purpose = keyring.decrypt(&envelope, other_purpose);

	// Assert
	assert_eq!(as_other_user.unwrap_err(), TokenCryptoError::Decrypt);
	assert_eq!(as_other_purpose.unwrap_err(), TokenCryptoError::Decrypt);
}

#[rstest]
fn sr_06_tampered_envelope_is_rejected(context: TokenContext) {
	// Arrange
	let keyring = keyring_with(Some(KEY_A), "", None).unwrap();
	let envelope = keyring
		.encrypt(&SecretString::new("ghu_secret"), context)
		.unwrap();
	let (head, body) = envelope.rsplit_once('.').unwrap();
	let mut flipped = body.to_owned();
	let replacement = if flipped.ends_with('A') { "B" } else { "A" };
	flipped.replace_range(flipped.len() - 1.., replacement);
	let tampered = format!("{head}.{flipped}");

	// Act
	let result = keyring.decrypt(&tampered, context);

	// Assert
	assert_eq!(result.unwrap_err(), TokenCryptoError::Decrypt);
}

#[rstest]
#[case::wrong_version("v2.primary.AAAA")]
#[case::missing_parts("v1.primary")]
#[case::extra_parts("v1.primary.AAAA.AAAA")]
#[case::not_base64("v1.primary.!!!")]
#[case::too_short("v1.primary.AAAA")]
fn sr_06_malformed_envelopes_are_rejected(context: TokenContext, #[case] envelope: &str) {
	// Arrange
	let keyring = keyring_with(Some(KEY_A), "", None).unwrap();

	// Act
	let result = keyring.decrypt(envelope, context);

	// Assert
	assert_eq!(result.unwrap_err(), TokenCryptoError::MalformedEnvelope);
}

#[rstest]
#[case::not_base64("not base64 at all")]
#[case::wrong_length("AAAA")]
#[case::unexpanded_placeholder("${REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY}")]
fn sr_06_malformed_key_fails_keyring_construction(#[case] key: &str) {
	// Arrange / Act
	let result = keyring_with(Some(key), "", None);

	// Assert
	assert_eq!(result.unwrap_err(), TokenCryptoError::MalformedKey);
}

#[rstest]
fn sr_06_derived_key_is_used_when_no_dedicated_key_is_configured(context: TokenContext) {
	// Arrange
	let keyring = keyring_with(None, "", None).unwrap();
	let other_secret = TokenKeyring::from_settings(
		TokenKeyringSettings {
			key: None,
			key_id: "",
			retired_keys: None,
		},
		"a-different-core-secret-key-value-000000",
	)
	.unwrap();

	// Act
	let envelope = keyring
		.encrypt(&SecretString::new("ghu_secret"), context)
		.unwrap();

	// Assert
	assert!(keyring.active_key_id().starts_with("core-"));
	assert_ne!(keyring.active_key_id(), other_secret.active_key_id());
	assert_eq!(
		other_secret.decrypt(&envelope, context).unwrap_err(),
		TokenCryptoError::UnknownKey {
			key_id: keyring.active_key_id().to_owned()
		}
	);
}

#[rstest]
fn sr_06_missing_key_material_fails_keyring_construction() {
	// Arrange / Act
	let result = TokenKeyring::from_settings(
		TokenKeyringSettings {
			key: None,
			key_id: "",
			retired_keys: None,
		},
		"",
	);

	// Assert
	assert_eq!(result.unwrap_err(), TokenCryptoError::MissingKeyMaterial);
}

#[rstest]
fn sr_06_rotation_keeps_old_values_readable_through_the_retired_list(context: TokenContext) {
	// Arrange
	let before_rotation = keyring_with(Some(KEY_A), "2025", None).unwrap();
	let envelope = before_rotation
		.encrypt(&SecretString::new("ghu_old"), context)
		.unwrap();
	let after_rotation = keyring_with(Some(KEY_B), "2026", Some(&format!("2025:{KEY_A}"))).unwrap();

	// Act
	let opened = after_rotation.decrypt(&envelope, context).unwrap();
	let fresh = after_rotation
		.encrypt(&SecretString::new("ghu_new"), context)
		.unwrap();

	// Assert
	assert_eq!(opened.expose_secret(), "ghu_old");
	assert!(fresh.starts_with("v1.2026."));
}

#[rstest]
#[case::duplicate_ids(Some(KEY_A), "same", Some(format!("same:{KEY_B}")), TokenCryptoError::DuplicateKeyId)]
#[case::key_id_without_key(None, "orphan", None, TokenCryptoError::KeyIdWithoutKey)]
#[case::bad_key_id(Some(KEY_A), "has.dot", None, TokenCryptoError::MalformedKeyId)]
#[case::bad_retired_entry(Some(KEY_A), "", Some("no-colon-here".to_owned()), TokenCryptoError::MalformedRetiredKeys)]
fn sr_06_inconsistent_key_configuration_is_rejected(
	#[case] key: Option<&str>,
	#[case] key_id: &str,
	#[case] retired: Option<String>,
	#[case] expected: TokenCryptoError,
) {
	// Arrange / Act
	let result = keyring_with(key, key_id, retired.as_deref());

	// Assert
	assert_eq!(result.unwrap_err(), expected);
}

#[rstest]
fn sr_102_keyring_and_tokens_redact_debug_output(context: TokenContext) {
	// Arrange
	let keyring = keyring_with(Some(KEY_A), "", None).unwrap();
	let token = SecretString::new("ghu_never_printed");
	let envelope = keyring.encrypt(&token, context).unwrap();
	let opened = keyring.decrypt(&envelope, context).unwrap();

	// Act
	let rendered = format!("{keyring:?} {token:?} {token} {opened:?}");

	// Assert
	assert_eq!(
		rendered,
		"TokenKeyring { active_key_id: \"primary\", retired_key_ids: [], .. } \
		 SecretString([REDACTED]) [REDACTED] SecretString([REDACTED])"
	);
}

#[rstest]
fn sr_102_decryption_errors_name_no_key_material_or_token(context: TokenContext) {
	// Arrange
	let sealing = keyring_with(Some(KEY_A), "shared", None).unwrap();
	let opening = keyring_with(Some(KEY_B), "shared", None).unwrap();
	let envelope = sealing
		.encrypt(&SecretString::new("ghu_never_in_errors"), context)
		.unwrap();

	// Act
	let message = opening.decrypt(&envelope, context).unwrap_err().to_string();

	// Assert
	assert_eq!(message, "stored token could not be decrypted");
}
