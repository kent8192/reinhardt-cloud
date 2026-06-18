//! API key lifecycle: generate, verify, revoke, list.
//!
//! Plaintext tokens are generated with a CSPRNG and stored only as a SHA-256
//! hash. The plaintext is returned to the caller exactly once at creation.
//!
//! Verification security model: the middleware hashes the submitted plaintext
//! and looks up the resulting hash via a unique-index equality filter. Because
//! the lookup is a single-row index probe (not a candidate scan), timing
//! depends on the uniform index lookup and does not reveal anything useful to
//! an attacker; application-level constant-time comparison would add no value.

use chrono::{DateTime, Utc};
use rand::{TryRngCore, rngs::OsRng};
use reinhardt::db::orm::{Model, execution::convert_values, get_connection};
use reinhardt::query::prelude::{
	Alias, Expr, ExprTrait, PostgresQueryBuilder, Query, QueryBuilder,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::apps::auth::models::{ApiKey, User};

/// Token plaintext prefix — aids secret-scanner detection (GitHub PAT style).
const TOKEN_PREFIX: &str = "rct_";
/// CSPRNG entropy length in bytes (32 bytes -> 64 hex chars).
const TOKEN_ENTROPY_BYTES: usize = 32;
/// Length of the non-secret display prefix stored for listings.
const DISPLAY_PREFIX_LEN: usize = 12;

/// Errors returned by API key operations.
#[derive(Debug, thiserror::Error)]
pub enum ApiKeyError {
	#[error("database error: {0}")]
	Database(String),
	#[error("user not found: {0}")]
	UserNotFound(String),
}

/// Generate a new API key. Returns `(plaintext, model)`.
///
/// The plaintext is shown once to the caller and never persisted; only its
/// SHA-256 hash is stored.
pub async fn generate_api_key(
	user_id: Uuid,
	label: String,
	expires_at: Option<DateTime<Utc>>,
) -> Result<(String, ApiKey), ApiKeyError> {
	// Confirm the user exists (fail fast with a clear error).
	let _user = User::objects()
		.filter(User::field_id().eq(user_id.to_string()))
		.first()
		.await
		.map_err(|e| ApiKeyError::Database(e.to_string()))?
		.ok_or_else(|| ApiKeyError::UserNotFound(user_id.to_string()))?;

	let mut entropy = [0u8; TOKEN_ENTROPY_BYTES];
	OsRng
		.try_fill_bytes(&mut entropy)
		.expect("CSPRNG fill failed");
	let plaintext = format!("{TOKEN_PREFIX}{}", hex::encode(entropy));
	let token_hash = sha256_hex(plaintext.as_bytes());
	let prefix: String = plaintext.chars().take(DISPLAY_PREFIX_LEN).collect();

	let api_key = ApiKey::build()
		.user(user_id)
		.label(label)
		.token_hash(token_hash)
		.prefix(prefix)
		.expires_at(expires_at)
		.revoked_at(None)
		.last_used_at(None)
		.finish();
	let created = ApiKey::objects()
		.create(&api_key)
		.await
		.map_err(|e| ApiKeyError::Database(e.to_string()))?;
	Ok((plaintext, created))
}

/// Verify a plaintext token. Returns `(User, api_key_id)` on success.
///
/// Single-row lookup by unique `token_hash`; rejects revoked / expired /
/// inactive-user tokens.
pub async fn verify_api_key(plaintext: &str) -> Option<(User, i64)> {
	let hash = sha256_hex(plaintext.as_bytes());
	let api_key = ApiKey::objects()
		.filter(ApiKey::field_token_hash().eq(hash))
		.first()
		.await
		.ok()
		.flatten()?;

	if api_key.revoked_at.is_some() {
		return None;
	}
	if let Some(exp) = api_key.expires_at
		&& exp <= Utc::now()
	{
		return None;
	}

	let user_id = *api_key.user_id();
	let user = User::objects()
		.filter(User::field_id().eq(user_id.to_string()))
		.first()
		.await
		.ok()
		.flatten()?;
	if !user.is_active {
		return None;
	}
	Some((user, api_key.id.unwrap_or_default()))
}

/// Soft-revoke a token by id.
pub async fn revoke_api_key(id: i64) -> Result<(), ApiKeyError> {
	let mut api_key = ApiKey::objects()
		.filter(ApiKey::field_id().eq(id))
		.first()
		.await
		.map_err(|e| ApiKeyError::Database(e.to_string()))?
		.ok_or_else(|| ApiKeyError::Database(format!("api key {id} not found")))?;
	api_key.revoked_at = Some(Utc::now());
	ApiKey::objects()
		.update(&api_key)
		.await
		.map_err(|e| ApiKeyError::Database(e.to_string()))?;
	Ok(())
}

/// List all keys for a user (any status; the caller decides display filtering).
pub async fn list_api_keys_for_user(user_id: Uuid) -> Result<Vec<ApiKey>, ApiKeyError> {
	ApiKey::objects()
		.filter(ApiKey::field_user_id().eq(user_id.to_string()))
		.order_by(&["id"])
		.all()
		.await
		.map_err(|e| ApiKeyError::Database(e.to_string()))
}

/// Record a successful verification timestamp. Fire-and-forget on the hot path
/// to avoid a write-per-request; callers spawn it without awaiting.
pub async fn touch_last_used(id: i64) {
	let Ok(conn) = get_connection().await else {
		return;
	};

	let mut stmt = Query::update();
	stmt.table(Alias::new("auth_api_keys"))
		.value(Alias::new("last_used_at"), Utc::now())
		.and_where(Expr::col(Alias::new("id")).eq(id))
		.and_where(Expr::col(Alias::new("revoked_at")).is_null());

	let builder = PostgresQueryBuilder::new();
	let (sql, values) = builder.build_update(&stmt);
	let params = convert_values(values);
	let _ = conn.execute(&sql, params).await;
}

fn sha256_hex(bytes: &[u8]) -> String {
	hex::encode(Sha256::digest(bytes))
}
