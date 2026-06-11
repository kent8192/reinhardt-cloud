//! ORM-backed implementation of `SocialAccountStorage`.
//!
//! Bridges `reinhardt-auth`'s framework-level `SocialAccount` struct (which
//! carries OAuth tokens, scopes, email, picture, etc.) and the dashboard's
//! local ORM model `crate::apps::auth::models::SocialAccount` (which only
//! persists the link itself).
//!
//! OAuth access tokens are persisted only through explicit helper methods
//! that encrypt the token before writing the ORM row. The trait-facing
//! account-linking API still returns tokenless accounts so generic social
//! auth callers cannot accidentally observe stored credentials.
//!
//! When the storage trait returns a `SocialAccount` to the framework, the
//! access token / email / display_name / picture fields are filled with
//! safe defaults (empty strings / `Vec::new()` / `None`) so that callers
//! that round-trip through `update()` cannot accidentally observe a token
//! value loaded from the database.
//!
//! No `#[injectable_factory]` conversion (kent8192/reinhardt-cloud#599):
//! `OrmSocialAccountStorage` is already a stateless unit struct that
//! reads no global settings or environment variables. Construction is
//! a single `OrmSocialAccountStorage::new()` call; adding a DI factory
//! around it would be pure ceremony with no observable benefit. If a
//! future refactor wants to inject this through `Depends<...>` to
//! decouple the OAuth view from the concrete storage type, the factory
//! can be added at that point without changing the type's surface.

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use reinhardt::auth::social::core::OAuthToken;
use reinhardt::auth::social::core::SocialAuthError;
use reinhardt::auth::social::storage::{SocialAccount, SocialAccountStorage};
use reinhardt::db::orm::{IntoPrimaryKey, Model};
use uuid::Uuid;

use crate::apps::auth::models::{SocialAccount as OrmSocialAccount, User};
use crate::apps::auth::services::oauth::token_crypto::{
	decrypt_access_token, encrypt_access_token,
};
/// Maps an ORM record to the framework `SocialAccount` shape with empty
/// token-bearing fields. The storage layer never returns persisted tokens.
fn orm_to_framework(orm: OrmSocialAccount) -> SocialAccount {
	SocialAccount {
		id: orm.id,
		user_id: *orm.user_id(),
		provider: orm.provider,
		provider_user_id: orm.provider_user_id,
		email: None,
		display_name: orm.provider_username,
		picture: None,
		access_token: String::new(),
		refresh_token: None,
		// `token_expires_at` is meaningful only when an access token is
		// being persisted, which we never do. The framework expects a
		// concrete `DateTime<Utc>`, so we report the unix epoch as the
		// "definitely expired" sentinel; refresh logic, if ever enabled,
		// must check `access_token.is_empty()` before consuming it.
		token_expires_at: Utc.timestamp_opt(0, 0).single().unwrap_or_default(),
		scopes: Vec::new(),
		created_at: orm.created_at,
		updated_at: orm.updated_at,
	}
}

/// Reinhardt ORM-backed storage for social-account linkages.
///
/// Singleton: holds no state — all calls go straight through to the
/// generated `OrmSocialAccount::objects()` query builder.
#[derive(Debug, Default, Clone, Copy)]
pub struct OrmSocialAccountStorage;

impl OrmSocialAccountStorage {
	/// Constructs a new storage handle. No connection state is held;
	/// every call resolves the database connection through the framework's
	/// DI container, exactly like the rest of the dashboard's ORM usage.
	pub const fn new() -> Self {
		Self
	}

	/// Persist encrypted token metadata for a linked provider account.
	pub async fn store_token_for_user(
		&self,
		user_id: Uuid,
		provider: &str,
		provider_user_id: &str,
		token: &OAuthToken,
	) -> Result<(), SocialAuthError> {
		let mut row = OrmSocialAccount::objects()
			.filter(OrmSocialAccount::field_user_id().eq(user_id.to_string()))
			.filter(OrmSocialAccount::field_provider().eq(provider.to_string()))
			.filter(OrmSocialAccount::field_provider_user_id().eq(provider_user_id.to_string()))
			.first()
			.await
			.map_err(|e| map_orm_err("store_token_for_user.lookup", e))?
			.ok_or_else(|| {
				SocialAuthError::Storage(format!(
					"Social account not found for provider {provider}: {provider_user_id}"
				))
			})?;
		row.encrypted_access_token = Some(
			encrypt_access_token(&token.access_token)
				.map_err(|e| SocialAuthError::Storage(e.to_string()))?,
		);
		row.token_expires_at = Some(token.expires_at);
		row.scopes = Some(token.scopes.join(" "));
		OrmSocialAccount::objects()
			.update(&row)
			.await
			.map_err(|e| map_orm_err("store_token_for_user.update", e))?;
		Ok(())
	}

	/// Load and decrypt a stored provider access token for a dashboard user.
	pub async fn access_token_for_user(
		&self,
		user_id: Uuid,
		provider: &str,
	) -> Result<Option<String>, SocialAuthError> {
		let row = OrmSocialAccount::objects()
			.filter(OrmSocialAccount::field_user_id().eq(user_id.to_string()))
			.filter(OrmSocialAccount::field_provider().eq(provider.to_string()))
			.first()
			.await
			.map_err(|e| map_orm_err("access_token_for_user.lookup", e))?;
		let Some(row) = row else {
			return Ok(None);
		};
		let Some(encrypted) = row.encrypted_access_token else {
			return Ok(None);
		};
		decrypt_access_token(&encrypted)
			.map(Some)
			.map_err(|e| SocialAuthError::Storage(e.to_string()))
	}
}

fn map_orm_err(context: &'static str, err: impl std::fmt::Display) -> SocialAuthError {
	SocialAuthError::Storage(format!("{context}: {err}"))
}

impl IntoPrimaryKey<User> for &SocialAccount {
	fn into_primary_key(self) -> Uuid {
		self.user_id
	}
}

#[async_trait]
impl SocialAccountStorage for OrmSocialAccountStorage {
	async fn find_by_provider_and_uid(
		&self,
		provider: &str,
		provider_user_id: &str,
	) -> Result<Option<SocialAccount>, SocialAuthError> {
		let row = OrmSocialAccount::objects()
			.filter(OrmSocialAccount::field_provider().eq(provider.to_string()))
			.filter(OrmSocialAccount::field_provider_user_id().eq(provider_user_id.to_string()))
			.first()
			.await
			.map_err(|e| map_orm_err("find_by_provider_and_uid", e))?;
		Ok(row.map(orm_to_framework))
	}

	async fn find_by_user(&self, user_id: Uuid) -> Result<Vec<SocialAccount>, SocialAuthError> {
		let rows = OrmSocialAccount::objects()
			.filter(OrmSocialAccount::field_user_id().eq(user_id.to_string()))
			.all()
			.await
			.map_err(|e| map_orm_err("find_by_user", e))?;
		Ok(rows.into_iter().map(orm_to_framework).collect())
	}

	async fn create(&self, account: SocialAccount) -> Result<SocialAccount, SocialAuthError> {
		let orm = OrmSocialAccount::build()
			.id(account.id)
			.user(&account)
			.provider(account.provider.clone())
			.provider_user_id(account.provider_user_id.clone())
			.provider_username(account.display_name.clone())
			.encrypted_access_token(None)
			.token_expires_at(None)
			.scopes(None)
			.created_at(account.created_at)
			.updated_at(account.updated_at)
			.finish();
		let created = OrmSocialAccount::objects()
			.create(&orm)
			.await
			.map_err(|e| map_orm_err("create", e))?;
		Ok(orm_to_framework(created))
	}

	async fn update(&self, account: SocialAccount) -> Result<SocialAccount, SocialAuthError> {
		// Only the link metadata is persisted; the token, scope, email and
		// picture fields are intentionally not written back. Mirror the
		// framework In-memory impl by treating "row missing" as an error.
		let exists = OrmSocialAccount::objects()
			.filter(OrmSocialAccount::field_id().eq(account.id.to_string()))
			.first()
			.await
			.map_err(|e| map_orm_err("update.lookup", e))?;
		if exists.is_none() {
			return Err(SocialAuthError::Storage(format!(
				"Social account not found: {}",
				account.id
			)));
		}

		// Refresh updated_at so observers can detect the change even
		// though we ignored token-bearing fields.
		let updated_at = chrono::Utc::now();

		let mut orm = exists.expect("existing social account checked above");
		orm.provider = account.provider.clone();
		orm.provider_user_id = account.provider_user_id.clone();
		orm.provider_username = account.display_name.clone();
		orm.updated_at = updated_at;
		let saved = OrmSocialAccount::objects()
			.update(&orm)
			.await
			.map_err(|e| map_orm_err("update", e))?;
		Ok(orm_to_framework(saved))
	}

	async fn delete(&self, id: Uuid) -> Result<(), SocialAuthError> {
		OrmSocialAccount::objects()
			.delete(id)
			.await
			.map_err(|e| map_orm_err("delete", e))?;
		Ok(())
	}
}
