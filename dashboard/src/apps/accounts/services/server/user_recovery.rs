//! Host-operator recovery of a User's sessions and activation.
//!
//! These are the tools the fail-closed paths point to. When re-pointing cannot
//! end a User's sessions it deactivates the User; when Staff revocation cannot
//! end them, they survive as ordinary sessions. Both leave sessions in Redis
//! that nobody presented while the User was inactive, and such a session becomes
//! valid again the moment the User is active. Reactivation therefore always ends
//! every session first, and refuses to reactivate when that fails.
//!
//! Both operations are reachable only from `manage` (`end-sessions`,
//! `reactivate-user`), never from a request: the admin site is no recovery path
//! because it needs an active Staff User, and the User who needs recovering may
//! be the only one.

use reinhardt::core::exception::Error as OrmError;
use reinhardt::db::orm::Model;
use uuid::Uuid;

use crate::apps::accounts::models::User;
use crate::apps::accounts::services::server::sessions::SessionRevoker;
use crate::apps::accounts::services::server::users::find_by_github_user_id;
use crate::audit::{ActorKind, AuditEvent, Outcome};
use crate::persisted_time::persisted_now;

/// Why sessions were not ended or a User was not reactivated.
#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
	/// The numeric GitHub user ID is not a valid one.
	#[error("the GitHub user ID must be a positive number")]
	InvalidGithubUserId,
	/// No User has this GitHub user ID.
	#[error("no User has this GitHub user ID")]
	UnknownUser,
	/// The sessions could not be ended. When reactivating, the User was left
	/// inactive: reactivating without ending them could revive old sessions.
	#[error("the User's sessions could not be ended ({0}); nothing else was changed")]
	SessionsNotEnded(String),
	/// The database refused the operation.
	#[error("user storage failed: {0}")]
	Storage(String),
}

impl From<OrmError> for RecoveryError {
	fn from(error: OrmError) -> Self {
		Self::Storage(error.to_string())
	}
}

/// How a reactivation ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactivateOutcome {
	/// Every session was ended (`sessions_ended` were live) and the User is
	/// active again.
	Reactivated {
		/// The User.
		user_id: Uuid,
		/// How many live sessions were destroyed.
		sessions_ended: usize,
	},
	/// The User was already active; nothing changed.
	Unchanged {
		/// The User.
		user_id: Uuid,
	},
}

async fn find(github_user_id: i64) -> Result<User, RecoveryError> {
	if github_user_id <= 0 {
		return Err(RecoveryError::InvalidGithubUserId);
	}
	find_by_github_user_id(github_user_id)
		.await
		.map_err(|error| RecoveryError::Storage(error.to_string()))?
		.ok_or(RecoveryError::UnknownUser)
}

/// End every browser session of the User with `github_user_id`; returns how many
/// were live.
///
/// Audited as `accounts.end_sessions.succeeded` / `.denied` (invalid or unknown
/// ID) / `.failed` (Redis or the database).
///
/// # Errors
///
/// Returns [`RecoveryError`] when the ID is invalid or unknown, or when the
/// sessions cannot be ended.
pub async fn end_sessions(
	github_user_id: i64,
	sessions: &dyn SessionRevoker,
) -> Result<usize, RecoveryError> {
	let event = |name, outcome| {
		AuditEvent::new(name, ActorKind::HostOperator, outcome).github_user(github_user_id)
	};
	let user = match find(github_user_id).await {
		Ok(user) => user,
		Err(error) => {
			let (name, outcome, reason) = match &error {
				RecoveryError::InvalidGithubUserId => (
					"accounts.end_sessions.denied",
					Outcome::Denied,
					"invalid_github_user_id",
				),
				RecoveryError::UnknownUser => (
					"accounts.end_sessions.denied",
					Outcome::Denied,
					"unknown_user",
				),
				RecoveryError::SessionsNotEnded(_) | RecoveryError::Storage(_) => {
					("accounts.end_sessions.failed", Outcome::Failed, "storage")
				}
			};
			event(name, outcome).reason(reason).emit();
			return Err(error);
		}
	};
	match sessions.destroy_all_for_user(user.id).await {
		Ok(ended) => {
			event("accounts.end_sessions.succeeded", Outcome::Succeeded)
				.subject_user(user.id)
				.emit();
			Ok(ended)
		}
		Err(cause) => {
			event("accounts.end_sessions.failed", Outcome::Failed)
				.subject_user(user.id)
				.reason("sessions_not_ended")
				.emit();
			Err(RecoveryError::SessionsNotEnded(cause.to_string()))
		}
	}
}

/// Reactivate the User with `github_user_id`, ending every session first.
///
/// A User who is already active is left alone (`Unchanged`, no session
/// touched). Otherwise the sessions are ended, and only when that succeeded is
/// `is_active` set; if ending them fails the User stays inactive and the
/// command is refused. Audited as `accounts.reactivate.succeeded` (reason
/// `reactivated` or `unchanged`) / `.refused` (`sessions_not_ended`,
/// `unknown_user`, `invalid_github_user_id`) / `.failed` (`storage`).
///
/// # Errors
///
/// Returns [`RecoveryError`] when the ID is invalid or unknown, the sessions
/// cannot be ended, or the database fails.
pub async fn reactivate(
	github_user_id: i64,
	sessions: &dyn SessionRevoker,
) -> Result<ReactivateOutcome, RecoveryError> {
	let result = reactivate_inner(github_user_id, sessions).await;
	let event = |name, outcome| {
		AuditEvent::new(name, ActorKind::HostOperator, outcome).github_user(github_user_id)
	};
	match &result {
		Ok(ReactivateOutcome::Reactivated { user_id, .. }) => {
			event("accounts.reactivate.succeeded", Outcome::Succeeded)
				.subject_user(*user_id)
				.reason("reactivated")
				.emit();
		}
		Ok(ReactivateOutcome::Unchanged { user_id }) => {
			event("accounts.reactivate.succeeded", Outcome::Succeeded)
				.subject_user(*user_id)
				.reason("unchanged")
				.emit();
		}
		Err(error) => {
			let (name, outcome, reason) = match error {
				RecoveryError::InvalidGithubUserId => (
					"accounts.reactivate.refused",
					Outcome::Denied,
					"invalid_github_user_id",
				),
				RecoveryError::UnknownUser => (
					"accounts.reactivate.refused",
					Outcome::Denied,
					"unknown_user",
				),
				RecoveryError::SessionsNotEnded(_) => (
					"accounts.reactivate.refused",
					Outcome::Denied,
					"sessions_not_ended",
				),
				RecoveryError::Storage(_) => {
					("accounts.reactivate.failed", Outcome::Failed, "storage")
				}
			};
			event(name, outcome).reason(reason).emit();
		}
	}
	result
}

async fn reactivate_inner(
	github_user_id: i64,
	sessions: &dyn SessionRevoker,
) -> Result<ReactivateOutcome, RecoveryError> {
	let user = find(github_user_id).await?;
	if user.is_active {
		return Ok(ReactivateOutcome::Unchanged { user_id: user.id });
	}
	// Sessions first. An inactive User cannot sign in, so none can appear between
	// this step and the update below; a failure here must leave the User inactive.
	let sessions_ended = sessions
		.destroy_all_for_user(user.id)
		.await
		.map_err(|error| RecoveryError::SessionsNotEnded(error.to_string()))?;
	User::objects()
		.filter(User::field_id().eq(user.id))
		.update_fields([
			User::field_is_active().assign(true),
			User::field_updated_at().assign(persisted_now()),
		])
		.await?;
	Ok(ReactivateOutcome::Reactivated {
		user_id: user.id,
		sessions_ended,
	})
}
