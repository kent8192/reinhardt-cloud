//! ORM-backed implementation of `SocialAccountStorage`.
//!
//! Bridges `reinhardt-auth`'s framework-level `SocialAccount` struct (which
//! carries OAuth tokens, scopes, email, picture, etc.) and the dashboard's
//! local ORM model `crate::apps::auth::models::SocialAccount` (which only
//! persists the link itself).
//!
//! Token persistence is deliberately omitted: the access_token is exchanged
//! during callback only to fetch user info, then dropped. Out of scope for
//! this PR are encrypted at-rest storage of OAuth tokens and refresh-flow
//! support; see #428 for the policy and follow-up issues for tracking.
//!
//! When the storage trait returns a `SocialAccount` to the framework, the
//! token / scope / email / display_name / picture fields are filled with
//! safe defaults (empty strings / `Vec::new()` / `None`) so that callers
//! that round-trip through `update()` cannot accidentally observe a token
//! value loaded from the database.

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue, Model};
use reinhardt_auth::social::core::SocialAuthError;
use reinhardt_auth::social::storage::{SocialAccount, SocialAccountStorage};
use uuid::Uuid;

use crate::apps::auth::models::SocialAccount as OrmSocialAccount;

/// Maps an ORM record to the framework `SocialAccount` shape with empty
/// token-bearing fields. The storage layer never returns persisted tokens.
fn orm_to_framework(orm: OrmSocialAccount) -> SocialAccount {
	SocialAccount {
		id: orm.id,
		user_id: orm.user_id,
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
}

fn map_orm_err(context: &'static str, err: impl std::fmt::Display) -> SocialAuthError {
	SocialAuthError::Storage(format!("{context}: {err}"))
}

/// Field name for the ORM `id` column (no `field_id()` accessor exists).
const ID_FIELD: &str = "id";

#[async_trait]
impl SocialAccountStorage for OrmSocialAccountStorage {
	async fn find_by_provider_and_uid(
		&self,
		provider: &str,
		provider_user_id: &str,
	) -> Result<Option<SocialAccount>, SocialAuthError> {
		let row = OrmSocialAccount::objects()
			.filter(
				OrmSocialAccount::field_provider(),
				FilterOperator::Eq,
				FilterValue::String(provider.to_string()),
			)
			.filter(Filter::new(
				OrmSocialAccount::field_provider_user_id().name(),
				FilterOperator::Eq,
				FilterValue::String(provider_user_id.to_string()),
			))
			.first()
			.await
			.map_err(|e| map_orm_err("find_by_provider_and_uid", e))?;
		Ok(row.map(orm_to_framework))
	}

	async fn find_by_user(&self, user_id: Uuid) -> Result<Vec<SocialAccount>, SocialAuthError> {
		let rows = OrmSocialAccount::objects()
			.filter(
				OrmSocialAccount::field_user_id(),
				FilterOperator::Eq,
				FilterValue::String(user_id.to_string()),
			)
			.all()
			.await
			.map_err(|e| map_orm_err("find_by_user", e))?;
		Ok(rows.into_iter().map(orm_to_framework).collect())
	}

	async fn create(&self, account: SocialAccount) -> Result<SocialAccount, SocialAuthError> {
		let orm = OrmSocialAccount {
			id: account.id,
			user_id: account.user_id,
			provider: account.provider.clone(),
			provider_user_id: account.provider_user_id.clone(),
			provider_username: account.display_name.clone(),
			created_at: account.created_at,
			updated_at: account.updated_at,
		};
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
			.filter(
				ID_FIELD,
				FilterOperator::Eq,
				FilterValue::String(account.id.to_string()),
			)
			.first()
			.await
			.map_err(|e| map_orm_err("update.lookup", e))?;
		if exists.is_none() {
			return Err(SocialAuthError::Storage(format!(
				"Social account not found: {}",
				account.id
			)));
		}

		let orm = OrmSocialAccount {
			id: account.id,
			user_id: account.user_id,
			provider: account.provider.clone(),
			provider_user_id: account.provider_user_id.clone(),
			provider_username: account.display_name.clone(),
			created_at: account.created_at,
			// Refresh updated_at so observers can detect the change even
			// though we ignored token-bearing fields.
			updated_at: chrono::Utc::now(),
		};
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
