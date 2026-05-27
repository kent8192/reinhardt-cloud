//! Account-linking semantics for completed OAuth callbacks.
//!
//! Resolves a `(provider, claims)` pair into a local `User` row, creating
//! a fresh user when needed. Implements the four-way decision tree from
//! issue #428:
//!
//!   (a) Already-linked   — return the user pointed to by the existing
//!       social-account row.
//!   (b) Authenticated    — caller is logged in; attach the new provider
//!       link to the current session's user.
//!   (c) Email-match      — provider asserted `email_verified == true`
//!       AND a local user with the same email already exists; attach.
//!   (d) New user         — none of the above; create a brand-new user
//!       with `password = None` (OAuth-only) and link it.
//!
//! `email_verified == None` (e.g. GitHub when the API does not surface
//! verification state) is treated the same as `Some(false)` — strictly
//! safer, since it forbids automatically merging into an existing local
//! account that the OAuth user might not actually own.
//!
//! No `#[injectable_factory]` conversion (kent8192/reinhardt-cloud#599):
//! `link_or_create_user` is a pure ORM-driven decision tree. All inputs
//! (`storage`, `provider`, `claims`, `current_user`) arrive as function
//! parameters; nothing is read from global settings or env vars. The
//! `SocialAccountStorage` trait object is supplied by the caller, which
//! is where DI happens (today the caller hand-constructs
//! `OrmSocialAccountStorage::new()` per request). A future refactor
//! could elevate the storage to a DI-resolved service, but that change
//! lives in `services::oauth::storage`, not here.

use chrono::Utc;
use reinhardt::db::orm::Model;
use reinhardt_auth::social::core::claims::StandardClaims;
use reinhardt_auth::social::storage::{SocialAccount, SocialAccountStorage};
use uuid::Uuid;

use crate::apps::auth::models::User;

/// Errors that can surface from `link_or_create_user`.
#[derive(Debug, thiserror::Error)]
pub enum LinkError {
	/// The OAuth provider returned claims missing a field we require.
	#[error("provider claims missing required field: {0}")]
	MissingClaim(&'static str),
	/// The `SocialAccountStorage` returned an error.
	#[error("social-account storage error: {0}")]
	Storage(String),
	/// The user-table lookup or insert failed.
	#[error("user database error: {0}")]
	Database(String),
	/// An account with this email already exists, but we cannot auto-link
	/// it (the provider did not assert email_verified == true). Caller must
	/// sign in with the existing account first and link from there.
	#[error(
		"an account with email {email} already exists; sign in with your \
		 existing account first and link {provider} from your profile"
	)]
	EmailConflict { email: String, provider: String },
}

/// Resolve OAuth claims into a local `User`, linking or creating as needed.
///
/// `current_user` carries the already-authenticated session user when the
/// caller is initiating a *link* flow (route (b)); pass `None` for plain
/// social login.
pub async fn link_or_create_user(
	storage: &dyn SocialAccountStorage,
	provider: &str,
	claims: &StandardClaims,
	current_user: Option<User>,
) -> Result<User, LinkError> {
	if claims.sub.is_empty() {
		return Err(LinkError::MissingClaim("sub"));
	}

	// (a) Already linked.
	if let Some(link) = storage
		.find_by_provider_and_uid(provider, &claims.sub)
		.await
		.map_err(|e| LinkError::Storage(e.to_string()))?
	{
		return load_user_by_id(link.user_id).await;
	}

	// (b) Authenticated link.
	if let Some(user) = current_user {
		let user_id = user.id;
		create_link(storage, provider, claims, user_id).await?;
		return Ok(user);
	}

	// (c) Email match (only when provider asserts email_verified).
	if claims.email_verified == Some(true)
		&& let Some(email) = claims.email.as_ref()
	{
		let normalized = email.to_lowercase();
		let existing = User::objects()
			.filter(User::field_email().eq(normalized))
			.first()
			.await
			.map_err(|e| LinkError::Database(e.to_string()))?;
		if let Some(user) = existing {
			let user_id = user.id;
			create_link(storage, provider, claims, user_id).await?;
			return Ok(user);
		}
	}

	// (d) New user.
	let username = generate_unique_username(claims).await?;
	let email = claims.email.clone().unwrap_or_default().to_lowercase();

	// Defensive: if a user with this email already exists, we got here
	// only because path (c) declined to merge (provider did not assert
	// email_verified == true). Refuse rather than crash on the unique
	// constraint, and surface a message that guides the user to the
	// link-from-existing-account flow.
	if !email.is_empty() {
		let conflict = User::objects()
			.filter(User::field_email().eq(email.clone()))
			.first()
			.await
			.map_err(|e| LinkError::Database(e.to_string()))?;
		if conflict.is_some() {
			return Err(LinkError::EmailConflict {
				email,
				provider: provider.to_string(),
			});
		}
	}

	let new_user = User::new(
		username,
		email,
		claims.given_name.clone().unwrap_or_default(),
		claims.family_name.clone().unwrap_or_default(),
		None,  // password_hash: OAuth-only user has no usable password
		true,  // is_active
		false, // is_staff
		false, // is_superuser
	);
	let created = User::objects()
		.create(&new_user)
		.await
		.map_err(|e| LinkError::Database(e.to_string()))?;
	let user_id = created.id;
	create_link(storage, provider, claims, user_id).await?;
	Ok(created)
}

async fn load_user_by_id(user_id: Uuid) -> Result<User, LinkError> {
	User::objects()
		.filter(User::field_id().eq(user_id.to_string()))
		.first()
		.await
		.map_err(|e| LinkError::Database(e.to_string()))?
		.ok_or_else(|| LinkError::Database(format!("orphaned social link to user {user_id}")))
}

async fn create_link(
	storage: &dyn SocialAccountStorage,
	provider: &str,
	claims: &StandardClaims,
	user_id: Uuid,
) -> Result<(), LinkError> {
	let now = Utc::now();
	let acc = SocialAccount {
		id: Uuid::now_v7(),
		user_id,
		provider: provider.to_string(),
		provider_user_id: claims.sub.clone(),
		// Token-bearing fields are dropped on the way through
		// `OrmSocialAccountStorage` per SEC E1, but we still send the
		// available metadata so an in-memory storage in tests can
		// observe the inputs.
		email: claims.email.clone(),
		display_name: display_name_from_claims(claims),
		picture: claims.picture.clone(),
		access_token: String::new(),
		refresh_token: None,
		token_expires_at: now,
		scopes: Vec::new(),
		created_at: now,
		updated_at: now,
	};
	storage
		.create(acc)
		.await
		.map_err(|e| LinkError::Storage(e.to_string()))?;
	Ok(())
}

fn display_name_from_claims(claims: &StandardClaims) -> Option<String> {
	if let Some(login) = claims
		.additional_claims
		.get("login")
		.and_then(|v| v.as_str())
	{
		return Some(login.to_string());
	}
	claims.name.clone()
}

async fn generate_unique_username(claims: &StandardClaims) -> Result<String, LinkError> {
	let raw = display_name_from_claims(claims).unwrap_or_else(|| claims.sub.clone());
	let base = sanitize_username(&raw);
	if !username_exists(&base).await? {
		return Ok(base);
	}
	for i in 1..10_000 {
		let candidate = truncate_username(&format!("{base}_{i}"));
		if !username_exists(&candidate).await? {
			return Ok(candidate);
		}
	}
	Err(LinkError::Database(
		"could not generate a unique username after 10000 attempts".to_string(),
	))
}

async fn username_exists(name: &str) -> Result<bool, LinkError> {
	let hit = User::objects()
		.filter(User::field_username().eq(name.to_string()))
		.first()
		.await
		.map_err(|e| LinkError::Database(e.to_string()))?;
	Ok(hit.is_some())
}

/// Strip characters that the dashboard does not allow in usernames and
/// truncate to the column limit (150).
fn sanitize_username(input: &str) -> String {
	let cleaned: String = input
		.chars()
		.map(|c| {
			if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
				c
			} else {
				'_'
			}
		})
		.collect();
	let trimmed = cleaned.trim_matches(['_', '.', '-'].as_ref());
	let final_str = if trimmed.is_empty() { "user" } else { trimmed };
	truncate_username(final_str)
}

fn truncate_username(s: &str) -> String {
	s.chars().take(150).collect()
}
