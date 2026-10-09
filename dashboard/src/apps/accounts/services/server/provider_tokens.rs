//! Database-backed storage for GitHub provider tokens.
//!
//! [`OrmSocialAccountStorage`] implements the upstream `SocialAccountStorage`
//! trait on top of the `SocialAccount` model. Differences from a plain record
//! store follow from SR-06:
//!
//! - Tokens are encrypted before they reach the database ([`TokenKeyring`]).
//! - Ordinary reads (every trait method) return **tokenless** records: the
//!   access token is an empty string and the refresh token is `None`. Callers
//!   that need a token ask for it explicitly with
//!   [`OrmSocialAccountStorage::load_tokens`].
//! - Passing a tokenless record back to `update` therefore keeps the stored
//!   tokens instead of erasing them.
//!
//! The GitHub identity (numeric user ID, login, name, avatar, email) is read
//! from the owning `User`; this storage never writes it.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reinhardt::auth::social::core::SocialAuthError;
use reinhardt::auth::social::storage::{SocialAccount, SocialAccountStorage};
use reinhardt::conf::settings::secret_types::SecretString;
use reinhardt::db::orm::Model;
use uuid::Uuid;

use crate::apps::accounts::models::{SocialAccount as SocialAccountRow, User};
use crate::apps::accounts::services::server::token_crypto::{
	TokenContext, TokenCryptoError, TokenKeyring, TokenPurpose,
};

/// The provider name this storage serves.
pub const GITHUB_PROVIDER: &str = "github";

/// The GitHub tokens of one User, in plaintext. Held only for the duration of
/// a call; the secret wrappers redact `Debug` and `Display` output.
#[derive(Clone, Debug)]
pub struct ProviderTokens {
	/// User access token.
	pub access_token: SecretString,
	/// User refresh token, when GitHub issued one.
	pub refresh_token: Option<SecretString>,
	/// When the access token expires.
	pub access_token_expires_at: DateTime<Utc>,
	/// When the refresh token expires, when GitHub reports it.
	pub refresh_token_expires_at: Option<DateTime<Utc>>,
}

/// Failures of token storage.
#[derive(Debug, thiserror::Error)]
pub enum ProviderTokenError {
	/// Sealing or opening a token failed.
	#[error(transparent)]
	Crypto(#[from] TokenCryptoError),
	/// The User the tokens belong to does not exist.
	#[error("user not found")]
	UserNotFound,
	/// The database refused the operation.
	#[error("provider token storage failed: {0}")]
	Storage(String),
}

impl ProviderTokenError {
	fn storage(error: impl std::fmt::Display) -> Self {
		Self::Storage(error.to_string())
	}
}

impl From<ProviderTokenError> for SocialAuthError {
	fn from(error: ProviderTokenError) -> Self {
		Self::Storage(error.to_string())
	}
}

/// ORM-backed `SocialAccountStorage` with encrypted token columns.
#[derive(Clone, Debug)]
pub struct OrmSocialAccountStorage {
	keyring: Arc<TokenKeyring>,
}

impl OrmSocialAccountStorage {
	/// Create a storage that seals tokens with `keyring`.
	#[must_use]
	pub fn new(keyring: Arc<TokenKeyring>) -> Self {
		Self { keyring }
	}

	/// Store the tokens of `user_id`, replacing any previous ones.
	///
	/// Both tokens and both expiries are written in one statement, so a
	/// rotated GitHub App refresh token is never persisted half-way.
	///
	/// # Errors
	///
	/// Returns an error when the User does not exist, sealing fails, or the
	/// database refuses the write.
	pub async fn store_tokens(
		&self,
		user_id: Uuid,
		tokens: &ProviderTokens,
	) -> Result<(), ProviderTokenError> {
		self.find_user(user_id).await?;
		let encrypted_access_token = self.keyring.encrypt(
			&tokens.access_token,
			TokenContext {
				user_id,
				purpose: TokenPurpose::Access,
			},
		)?;
		let encrypted_refresh_token = tokens
			.refresh_token
			.as_ref()
			.map(|token| {
				self.keyring.encrypt(
					token,
					TokenContext {
						user_id,
						purpose: TokenPurpose::Refresh,
					},
				)
			})
			.transpose()?;

		if let Some(row) = find_row_by_user(user_id).await? {
			update_row_tokens(
				row.id,
				&encrypted_access_token,
				encrypted_refresh_token.as_deref(),
				tokens,
			)
			.await
		} else {
			let new_row = SocialAccountRow::build()
				.user(user_id)
				.encrypted_access_token(encrypted_access_token.clone())
				.encrypted_refresh_token(encrypted_refresh_token.clone())
				.access_token_expires_at(tokens.access_token_expires_at)
				.refresh_token_expires_at(tokens.refresh_token_expires_at)
				.finish();
			match SocialAccountRow::objects().create(&new_row).await {
				Ok(_) => Ok(()),
				Err(create_error) => {
					// A concurrent store for the same User won the unique
					// constraint; overwrite its row with ours.
					match find_row_by_user(user_id).await? {
						Some(row) => {
							update_row_tokens(
								row.id,
								&encrypted_access_token,
								encrypted_refresh_token.as_deref(),
								tokens,
							)
							.await
						}
						None => Err(ProviderTokenError::storage(create_error)),
					}
				}
			}
		}
	}

	/// Load and decrypt the tokens of `user_id`.
	///
	/// This is the only read path that returns token text.
	///
	/// # Errors
	///
	/// Returns an error when the query fails or a stored token cannot be
	/// decrypted (wrong key, wrong owner, or tampering).
	pub async fn load_tokens(
		&self,
		user_id: Uuid,
	) -> Result<Option<ProviderTokens>, ProviderTokenError> {
		let Some(row) = find_row_by_user(user_id).await? else {
			return Ok(None);
		};
		let access_token = self.keyring.decrypt(
			&row.encrypted_access_token,
			TokenContext {
				user_id,
				purpose: TokenPurpose::Access,
			},
		)?;
		let refresh_token = row
			.encrypted_refresh_token
			.as_deref()
			.map(|envelope| {
				self.keyring.decrypt(
					envelope,
					TokenContext {
						user_id,
						purpose: TokenPurpose::Refresh,
					},
				)
			})
			.transpose()?;
		Ok(Some(ProviderTokens {
			access_token,
			refresh_token,
			access_token_expires_at: row.access_token_expires_at,
			refresh_token_expires_at: row.refresh_token_expires_at,
		}))
	}

	async fn find_user(&self, user_id: Uuid) -> Result<User, ProviderTokenError> {
		User::objects()
			.filter(User::field_id().eq(user_id))
			.first()
			.await
			.map_err(ProviderTokenError::storage)?
			.ok_or(ProviderTokenError::UserNotFound)
	}

	async fn to_record(&self, row: SocialAccountRow) -> Result<SocialAccount, ProviderTokenError> {
		let user = self.find_user(row.user_id()).await?;
		Ok(SocialAccount {
			id: row.id,
			user_id: user.id,
			provider: GITHUB_PROVIDER.to_owned(),
			provider_user_id: user.github_user_id.to_string(),
			email: user.email,
			display_name: Some(user.display_name),
			picture: user.avatar_url,
			// Tokenless on purpose (SR-06); see the module documentation.
			access_token: String::new(),
			refresh_token: None,
			token_expires_at: row.access_token_expires_at,
			scopes: Vec::new(),
			created_at: row.created_at,
			updated_at: row.updated_at,
		})
	}
}

async fn find_row_by_user(user_id: Uuid) -> Result<Option<SocialAccountRow>, ProviderTokenError> {
	SocialAccountRow::objects()
		.filter(SocialAccountRow::field_user_id().eq(user_id))
		.first()
		.await
		.map_err(ProviderTokenError::storage)
}

async fn update_row_tokens(
	row_id: Uuid,
	encrypted_access_token: &str,
	encrypted_refresh_token: Option<&str>,
	tokens: &ProviderTokens,
) -> Result<(), ProviderTokenError> {
	SocialAccountRow::objects()
		.filter(SocialAccountRow::field_id().eq(row_id))
		.update_fields([
			SocialAccountRow::field_encrypted_access_token()
				.assign(encrypted_access_token.to_owned()),
			SocialAccountRow::field_encrypted_refresh_token()
				.assign(encrypted_refresh_token.map(str::to_owned)),
			SocialAccountRow::field_access_token_expires_at()
				.assign(tokens.access_token_expires_at),
			SocialAccountRow::field_refresh_token_expires_at()
				.assign(tokens.refresh_token_expires_at),
			SocialAccountRow::field_updated_at().assign(Utc::now()),
		])
		.await
		.map(|_| ())
		.map_err(ProviderTokenError::storage)
}

fn ensure_github(provider: &str) -> Result<(), SocialAuthError> {
	if provider == GITHUB_PROVIDER {
		Ok(())
	} else {
		Err(SocialAuthError::Storage(format!(
			"unsupported provider {provider:?}"
		)))
	}
}

#[async_trait]
impl SocialAccountStorage for OrmSocialAccountStorage {
	async fn find_by_provider_and_uid(
		&self,
		provider: &str,
		provider_user_id: &str,
	) -> Result<Option<SocialAccount>, SocialAuthError> {
		if provider != GITHUB_PROVIDER {
			return Ok(None);
		}
		let Ok(github_user_id) = provider_user_id.parse::<i64>() else {
			return Ok(None);
		};
		let Some(user) =
			crate::apps::accounts::services::server::users::find_by_github_user_id(github_user_id)
				.await
				.map_err(|error| SocialAuthError::Storage(error.to_string()))?
		else {
			return Ok(None);
		};
		match find_row_by_user(user.id).await? {
			Some(row) => Ok(Some(self.to_record(row).await?)),
			None => Ok(None),
		}
	}

	async fn find_by_user(&self, user_id: Uuid) -> Result<Vec<SocialAccount>, SocialAuthError> {
		match find_row_by_user(user_id).await? {
			Some(row) => Ok(vec![self.to_record(row).await?]),
			None => Ok(Vec::new()),
		}
	}

	async fn create(&self, account: SocialAccount) -> Result<SocialAccount, SocialAuthError> {
		ensure_github(&account.provider)?;
		let user = self.find_user(account.user_id).await?;
		if user.github_user_id.to_string() != account.provider_user_id {
			return Err(SocialAuthError::Storage(
				"provider user id does not match the user's GitHub identity".to_owned(),
			));
		}
		if find_row_by_user(user.id).await?.is_some() {
			return Err(SocialAuthError::Storage(
				"a social account already exists for this user".to_owned(),
			));
		}
		if account.access_token.is_empty() {
			return Err(SocialAuthError::Storage(
				"a new social account requires an access token".to_owned(),
			));
		}
		self.store_tokens(user.id, &tokens_of(&account)).await?;
		let row = find_row_by_user(user.id)
			.await?
			.ok_or_else(|| SocialAuthError::Storage("social account vanished".to_owned()))?;
		Ok(self.to_record(row).await?)
	}

	async fn update(&self, account: SocialAccount) -> Result<SocialAccount, SocialAuthError> {
		ensure_github(&account.provider)?;
		let row = SocialAccountRow::objects()
			.filter(SocialAccountRow::field_id().eq(account.id))
			.first()
			.await
			.map_err(ProviderTokenError::storage)?
			.ok_or_else(|| {
				SocialAuthError::Storage(format!("Social account not found: {}", account.id))
			})?;
		if !account.access_token.is_empty() {
			self.store_tokens(row.user_id(), &tokens_of(&account))
				.await?;
		}
		let row = SocialAccountRow::objects()
			.filter(SocialAccountRow::field_id().eq(account.id))
			.first()
			.await
			.map_err(ProviderTokenError::storage)?
			.ok_or_else(|| {
				SocialAuthError::Storage(format!("Social account not found: {}", account.id))
			})?;
		Ok(self.to_record(row).await?)
	}

	async fn delete(&self, id: Uuid) -> Result<(), SocialAuthError> {
		let existing = SocialAccountRow::objects()
			.filter(SocialAccountRow::field_id().eq(id))
			.first()
			.await
			.map_err(ProviderTokenError::storage)?;
		if existing.is_none() {
			return Err(SocialAuthError::Storage(format!(
				"Social account not found: {id}"
			)));
		}
		SocialAccountRow::objects()
			.delete(id)
			.await
			.map_err(ProviderTokenError::storage)?;
		Ok(())
	}
}

/// Convert the plaintext of an upstream record into the storage shape. The
/// upstream record is consumed by its caller right after, so the plaintext does
/// not outlive the call.
fn tokens_of(account: &SocialAccount) -> ProviderTokens {
	ProviderTokens {
		access_token: SecretString::new(account.access_token.clone()),
		refresh_token: account.refresh_token.clone().map(SecretString::new),
		access_token_expires_at: account.token_expires_at,
		refresh_token_expires_at: None,
	}
}
