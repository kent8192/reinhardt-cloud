//! The `[accounts]` settings fragment.
//!
//! Every value is a string so that deployed profiles can feed it from the
//! environment through `${VAR:-default}` interpolation (the settings loader
//! coerces interpolated text only into string fields without further help).
//! List-valued settings are comma-separated.

use reinhardt::conf::settings::fragment::SettingsValidation;
use reinhardt::conf::settings::profile::Profile;
use reinhardt::conf::settings::secret_types::SecretString;
use reinhardt::conf::settings::validation::{ValidationError, ValidationResult};
use reinhardt::settings;
use serde::{Deserialize, Serialize};

use crate::apps::accounts::services::server::sign_up_policy::{SignUpPolicy, SignUpPolicyError};
use crate::apps::accounts::services::server::token_crypto::{
	TokenCryptoError, TokenKeyring, TokenKeyringSettings,
};

/// Settings of the accounts application.
///
/// | Key | Environment variable (committed profiles) |
/// |-----|-------------------------------------------|
/// | `sign_up_policy` | `REINHARDT_CLOUD_SIGN_UP_POLICY` |
/// | `sign_up_allowed_github_user_ids` | `REINHARDT_CLOUD_SIGN_UP_ALLOWED_GITHUB_USER_IDS` |
/// | `sign_up_allowed_github_organization_ids` | `REINHARDT_CLOUD_SIGN_UP_ALLOWED_GITHUB_ORGANIZATION_IDS` |
/// | `token_encryption_key` | `REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY` |
/// | `token_encryption_key_id` | `REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY_ID` |
/// | `token_encryption_retired_keys` | `REINHARDT_CLOUD_TOKEN_ENCRYPTION_RETIRED_KEYS` |
#[settings(fragment = true, section = "accounts", validate = false)]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AccountsSettings {
	/// Sign-up policy name: `open`, `allowlist`, or `invite_only`. Anything
	/// else, including an empty value, resolves to `invite_only` (SR-19).
	#[serde(default)]
	pub sign_up_policy: String,

	/// Comma-separated numeric GitHub user IDs admitted under `allowlist`.
	#[serde(default)]
	pub sign_up_allowed_github_user_ids: String,

	/// Comma-separated numeric GitHub organization IDs whose members are
	/// admitted under `allowlist`. IDs rather than logins, because a renamed
	/// organization's old login can be claimed by someone else (SR-19).
	#[serde(default)]
	pub sign_up_allowed_github_organization_ids: String,

	/// Base64 of a dedicated 32-byte provider-token encryption key. When empty,
	/// the key is derived from `core.secret_key` with a domain-separated KDF.
	#[serde(default)]
	pub token_encryption_key: Option<SecretString>,

	/// Identifier stored beside every ciphertext produced with the key above.
	#[serde(default)]
	pub token_encryption_key_id: String,

	/// Comma-separated `id:base64` pairs of retired keys, kept to decrypt
	/// tokens written before a rotation.
	#[serde(default)]
	pub token_encryption_retired_keys: Option<SecretString>,
}

impl AccountsSettings {
	/// Build the sign-up policy these settings describe.
	///
	/// # Errors
	///
	/// Returns an error when an allowlist entry is not a numeric ID.
	pub fn sign_up_policy(&self) -> Result<SignUpPolicy, SignUpPolicyError> {
		SignUpPolicy::from_settings(
			&self.sign_up_policy,
			&self.sign_up_allowed_github_user_ids,
			&self.sign_up_allowed_github_organization_ids,
		)
	}

	/// Build the provider-token keyring.
	///
	/// `secret_key` is `core.secret_key`, the fallback key material.
	///
	/// # Errors
	///
	/// Returns an error when a configured key is malformed (SR-06).
	pub fn token_keyring(&self, secret_key: &str) -> Result<TokenKeyring, TokenCryptoError> {
		TokenKeyring::from_settings(
			TokenKeyringSettings {
				key: self.token_encryption_key.as_ref(),
				key_id: &self.token_encryption_key_id,
				retired_keys: self.token_encryption_retired_keys.as_ref(),
			},
			secret_key,
		)
	}
}

impl SettingsValidation for AccountsSettings {
	fn validate(&self, _profile: &Profile) -> ValidationResult {
		self.sign_up_policy()
			.map_err(|error| ValidationError::InvalidValue {
				key: "accounts.sign_up_allowed_*".to_owned(),
				message: error.to_string(),
			})?;
		Ok(())
	}
}
