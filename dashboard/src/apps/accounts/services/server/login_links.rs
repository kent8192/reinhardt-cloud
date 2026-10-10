//! Login Links: single-use, short-lived sign-in URLs for break-glass access
//! (SR-16, SR-17, SR-18).
//!
//! - **Issuance** is only reachable from `manage create-login-link`; nothing in
//!   the HTTP, WebSocket, or gRPC surface calls [`issue`]. The target must be an
//!   existing, active User: a Login Link never creates one (SR-18).
//! - **The secret** is 256 random bits from the operating system's generator,
//!   shown once to the operator and stored only as its SHA-256 digest. It is a
//!   [`LoginLinkSecret`], which redacts itself in `Debug` and is never handed to
//!   `tracing`.
//! - **Lifetime** is fixed at issuance and can never exceed [`MAX_LIFETIME`], a
//!   constant: no setting can raise it (SR-17).
//! - **Consumption** is one conditional `UPDATE` (unused AND not expired AND the
//!   digest matches). Exactly one concurrent caller sees one affected row, so a
//!   link cannot be consumed twice (SR-16). Every way of failing is the same
//!   [`ConsumeError::Rejected`]; the specific cause goes only to the audit log.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use rand::Rng;
use reinhardt::db::orm::Model;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::apps::accounts::models::{LoginLink, User};
use crate::apps::accounts::services::server::users::{UserError, find_by_github_user_id};
use crate::audit::{ActorKind, AuditEvent, Outcome};
use crate::persisted_time::{persisted_now, to_persisted};

/// The longest a Login Link may live.
///
/// SR-17 proposes 15 minutes and asks that configuration cannot exceed the
/// ceiling. A constant satisfies that by construction: the operator picks a
/// shorter lifetime per link, never a longer one. Fifteen minutes covers an
/// operator copying a URL into a browser or a CI job; anything longer would be
/// a standing credential.
pub const MAX_LIFETIME: Duration = Duration::from_secs(15 * 60);

/// The lifetime used when the operator does not choose one.
pub const DEFAULT_LIFETIME: Duration = Duration::from_secs(10 * 60);

/// Longest secret [`consume`] will hash. The real ones are 43 characters; the
/// limit only keeps a hostile request from making the server hash a large body.
const MAX_SECRET_LEN: usize = 256;

/// Entropy of a Login Link secret, in bytes (256 bits, above SR-18's 128).
const SECRET_BYTES: usize = 32;

/// The secret half of a Login Link.
///
/// Deliberately not `Display` and with a redacting `Debug`: the only way out is
/// [`LoginLinkSecret::expose`], used once to print the URL.
#[derive(Clone, PartialEq, Eq)]
pub struct LoginLinkSecret(String);

impl LoginLinkSecret {
	fn generate() -> Self {
		let mut bytes = [0_u8; SECRET_BYTES];
		rand::rng().fill(&mut bytes);
		Self(URL_SAFE_NO_PAD.encode(bytes))
	}

	/// Wrap text received from a browser. It is not trusted until consumed.
	#[must_use]
	pub fn from_input(value: &str) -> Self {
		Self(value.to_owned())
	}

	/// The secret text, for building the URL the operator receives.
	#[must_use]
	pub fn expose(&self) -> &str {
		&self.0
	}

	/// Lowercase hex SHA-256 of the secret: the only form that is stored.
	fn digest(&self) -> String {
		use std::fmt::Write;
		Sha256::digest(self.0.as_bytes())
			.iter()
			.fold(String::with_capacity(64), |mut out, byte| {
				let _ = write!(out, "{byte:02x}");
				out
			})
	}
}

impl std::fmt::Debug for LoginLinkSecret {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("LoginLinkSecret(<redacted>)")
	}
}

/// A Login Link that was just issued.
#[derive(Debug, Clone)]
pub struct IssuedLoginLink {
	/// The secret to put in the URL. Shown once.
	pub secret: LoginLinkSecret,
	/// The User the link signs in.
	pub user_id: Uuid,
	/// When the link stops working.
	pub expires_at: DateTime<Utc>,
}

/// Why no Login Link was issued.
#[derive(Debug, thiserror::Error)]
pub enum IssueError {
	/// The numeric GitHub user ID is not a valid one.
	#[error("the GitHub user ID must be a positive number")]
	InvalidGithubUserId,
	/// The lifetime is zero or longer than [`MAX_LIFETIME`].
	#[error("the lifetime must be between one second and {} minutes", MAX_LIFETIME.as_secs() / 60)]
	InvalidLifetime,
	/// No User has this GitHub user ID. A Login Link never creates one.
	#[error("no User has this GitHub user ID; a Login Link cannot create a User")]
	UnknownUser,
	/// The User is deactivated.
	#[error("the User is deactivated")]
	InactiveUser,
	/// The database refused the operation.
	#[error("login link storage failed: {0}")]
	Storage(String),
}

/// Issue a Login Link for the User with `github_user_id`, valid for `lifetime`.
///
/// Audited as `accounts.login_link.issued` (or `.issue_denied` / `.issue_failed`)
/// with the host operator as the actor. The secret appears in no audit field.
///
/// # Errors
///
/// Returns [`IssueError`] when the lifetime is out of range, the User is unknown
/// or deactivated, or the database fails.
pub async fn issue(github_user_id: i64, lifetime: Duration) -> Result<IssuedLoginLink, IssueError> {
	let result = issue_inner(github_user_id, lifetime).await;
	let event = |name, outcome| AuditEvent::new(name, ActorKind::HostOperator, outcome);
	match &result {
		Ok(link) => event("accounts.login_link.issued", Outcome::Succeeded)
			.subject_user(link.user_id)
			.github_user(github_user_id)
			.emit(),
		Err(error) => {
			let (name, outcome, reason) = match error {
				IssueError::InvalidGithubUserId => (
					"accounts.login_link.issue_denied",
					Outcome::Denied,
					"invalid_github_user_id",
				),
				IssueError::InvalidLifetime => (
					"accounts.login_link.issue_denied",
					Outcome::Denied,
					"invalid_lifetime",
				),
				IssueError::UnknownUser => (
					"accounts.login_link.issue_denied",
					Outcome::Denied,
					"unknown_user",
				),
				IssueError::InactiveUser => (
					"accounts.login_link.issue_denied",
					Outcome::Denied,
					"user_inactive",
				),
				IssueError::Storage(_) => (
					"accounts.login_link.issue_failed",
					Outcome::Failed,
					"storage",
				),
			};
			event(name, outcome)
				.github_user(github_user_id)
				.reason(reason)
				.emit();
		}
	}
	result
}

async fn issue_inner(
	github_user_id: i64,
	lifetime: Duration,
) -> Result<IssuedLoginLink, IssueError> {
	if github_user_id <= 0 {
		return Err(IssueError::InvalidGithubUserId);
	}
	if lifetime.is_zero() || lifetime > MAX_LIFETIME {
		return Err(IssueError::InvalidLifetime);
	}
	let user = find_by_github_user_id(github_user_id)
		.await
		.map_err(storage)?
		.ok_or(IssueError::UnknownUser)?;
	if !user.is_active {
		return Err(IssueError::InactiveUser);
	}

	let secret = LoginLinkSecret::generate();
	let expires_at = to_persisted(
		persisted_now()
			+ chrono::Duration::from_std(lifetime).map_err(|_| IssueError::InvalidLifetime)?,
	);
	let row = LoginLink::build()
		.user(user.id)
		.token_hash(secret.digest())
		.expires_at(expires_at)
		.consumed_at(None)
		.finish();
	LoginLink::objects()
		.create(&row)
		.await
		.map_err(|error| IssueError::Storage(error.to_string()))?;
	Ok(IssuedLoginLink {
		secret,
		user_id: user.id,
		expires_at,
	})
}

fn storage(error: UserError) -> IssueError {
	IssueError::Storage(error.to_string())
}

/// Why a Login Link was not consumed.
#[derive(Debug, thiserror::Error)]
pub enum ConsumeError {
	/// The link is unknown, used, expired, or belongs to a User who may not sign
	/// in. Callers must not tell these apart (SR-16).
	#[error("the login link is not valid")]
	Rejected,
	/// The database refused the operation.
	#[error("login link storage failed: {0}")]
	Storage(String),
}

/// Consume `secret` and return the User it signs in.
///
/// The conditional update is the whole check: a used, expired, revoked, or
/// unknown link affects no row. A link of a User who has since been
/// deactivated is consumed (it is burned) and then rejected, so the answer is
/// the same as for any other bad link. Audited as `accounts.login_link.consumed`
/// or `.rejected` (with the private cause) with the person holding the link as
/// the actor. This function does not create a session; the caller does, through
/// `SessionService::replace`, exactly as GitHub sign-in does.
///
/// # Errors
///
/// Returns [`ConsumeError::Rejected`] for every invalid link and
/// [`ConsumeError::Storage`] when the database fails.
pub async fn consume(secret: &LoginLinkSecret) -> Result<User, ConsumeError> {
	match consume_inner(secret).await {
		Ok(user) => {
			AuditEvent::new(
				"accounts.login_link.consumed",
				ActorKind::User,
				Outcome::Succeeded,
			)
			.actor_user(user.id)
			.subject_user(user.id)
			.github_user(user.github_user_id)
			.emit();
			Ok(user)
		}
		Err((error, cause)) => {
			match &error {
				ConsumeError::Rejected => {
					let mut event = AuditEvent::new(
						"accounts.login_link.rejected",
						ActorKind::User,
						Outcome::Denied,
					)
					.reason(cause.code());
					if let Some(user) = cause.user_id() {
						event = event.subject_user(user);
					}
					event.emit();
				}
				ConsumeError::Storage(_) => AuditEvent::new(
					"accounts.login_link.consume_failed",
					ActorKind::User,
					Outcome::Failed,
				)
				.reason("storage")
				.emit(),
			}
			Err(error)
		}
	}
}

/// The private reason a link was rejected, for the audit log only.
#[derive(Debug, Clone, Copy)]
enum RejectCause {
	Malformed,
	Unknown,
	Used(Uuid),
	Expired(Uuid),
	UserInactive(Uuid),
	None,
}

impl RejectCause {
	const fn code(self) -> &'static str {
		match self {
			Self::Malformed => "malformed",
			Self::Unknown => "unknown",
			Self::Used(_) => "used",
			Self::Expired(_) => "expired",
			Self::UserInactive(_) => "user_inactive",
			Self::None => "storage",
		}
	}

	const fn user_id(self) -> Option<Uuid> {
		match self {
			Self::Used(id) | Self::Expired(id) | Self::UserInactive(id) => Some(id),
			Self::Malformed | Self::Unknown | Self::None => None,
		}
	}
}

async fn consume_inner(secret: &LoginLinkSecret) -> Result<User, (ConsumeError, RejectCause)> {
	let storage_failure = |error: String| (ConsumeError::Storage(error), RejectCause::None);
	let text = secret.expose();
	if text.is_empty() || text.len() > MAX_SECRET_LEN {
		return Err((ConsumeError::Rejected, RejectCause::Malformed));
	}
	let digest = secret.digest();
	let now = persisted_now();

	let consumed = LoginLink::objects()
		.filter(LoginLink::field_token_hash().eq(digest.clone()))
		.filter(LoginLink::field_consumed_at().is_null())
		.filter(LoginLink::field_expires_at().gt(now))
		.update_fields([LoginLink::field_consumed_at().assign(Some(now))])
		.await
		.map_err(|error| storage_failure(error.to_string()))?;

	let link = LoginLink::objects()
		.filter(LoginLink::field_token_hash().eq(digest))
		.first()
		.await
		.map_err(|error| storage_failure(error.to_string()))?;
	let Some(link) = link else {
		return Err((ConsumeError::Rejected, RejectCause::Unknown));
	};
	let user_id = link.user_id();
	if consumed != 1 {
		// Explain the rejection to the audit log only. `consumed_at` was set by
		// whoever won, or the link was already past its expiry.
		let cause = if link.consumed_at.is_some() {
			RejectCause::Used(user_id)
		} else {
			RejectCause::Expired(user_id)
		};
		return Err((ConsumeError::Rejected, cause));
	}

	let user = User::objects()
		.filter(User::field_id().eq(user_id))
		.first()
		.await
		.map_err(|error| storage_failure(error.to_string()))?;
	match user {
		Some(user) if user.is_active => Ok(user),
		Some(user) => Err((ConsumeError::Rejected, RejectCause::UserInactive(user.id))),
		None => Err((ConsumeError::Rejected, RejectCause::Unknown)),
	}
}

#[cfg(test)]
pub(crate) mod testing {
	//! Helpers for tests that need to create Login Link rows directly.

	use super::*;

	/// Insert a link row for `user_id` with explicit timestamps and return its
	/// secret, so tests can build expired or already-used links.
	pub(crate) async fn insert(
		user_id: Uuid,
		expires_at: DateTime<Utc>,
		consumed_at: Option<DateTime<Utc>>,
	) -> LoginLinkSecret {
		let secret = LoginLinkSecret::generate();
		let row = LoginLink::build()
			.user(user_id)
			.token_hash(secret.digest())
			.expires_at(to_persisted(expires_at))
			.consumed_at(consumed_at.map(to_persisted))
			.finish();
		LoginLink::objects()
			.create(&row)
			.await
			.expect("the link row should be created");
		secret
	}

	/// The stored digest of `secret`.
	pub(crate) fn digest(secret: &LoginLinkSecret) -> String {
		secret.digest()
	}
}
