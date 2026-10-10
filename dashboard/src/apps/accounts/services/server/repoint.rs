//! Moving a User to another GitHub account (SR-107).
//!
//! The only way to change which numeric GitHub user ID a User answers to. It is
//! reached only from `manage repoint-github-account`: no HTTP, WebSocket, or
//! gRPC endpoint calls it, and the admin site shows the identity as read-only.
//!
//! What moves and what does not:
//!
//! - The User row keeps its internal ID, so Memberships and Roles (which key on
//!   it) and the Staff flag stay. The profile copied from the old GitHub account
//!   (login, name, avatar, email) is replaced by the placeholder a new User gets
//!   until the new account signs in, so the old account's data does not linger.
//! - The stored provider tokens belong to the old GitHub identity and are
//!   deleted, as are the User's unused Login Links, which the operator issued
//!   for the old arrangement.
//! - Every session of the User ends. CLI Sessions (M2) key on the same User and
//!   must be revoked here too when they exist.
//!
//! The database changes happen in one transaction; the sessions are Redis and
//! cannot join it, so they are ended before and again after it. The first pass
//! aborts the operation if Redis is down, before anything changed; the second
//! closes the window in which the old account could have signed in again.
//!
//! If the second pass fails, the move has already committed and been audited.
//! The operation then fails closed: the User is deactivated, and the per-request
//! session check reads `is_active` from the database, so every leftover session
//! is refused without Redis. An operator reactivates the User (admin site, set
//! `is_active`) once Redis is back.

use reinhardt::core::exception::{DatabaseError, DatabaseErrorKind, Error as OrmError};
use reinhardt::db::orm::{Model, get_connection};
use uuid::Uuid;

use crate::apps::accounts::models::{LoginLink, SocialAccount, User};
use crate::apps::accounts::services::server::sessions::SessionRevoker;
use crate::apps::accounts::services::server::staff::placeholder_login;
use crate::apps::accounts::services::server::users::find_by_github_user_id;
use crate::audit::{ActorKind, AuditEvent, Outcome};
use crate::persisted_time::persisted_now;

/// A completed re-pointing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repointed {
	/// The User that moved.
	pub user_id: Uuid,
	/// How many live sessions the second pass ended (the first pass counts too,
	/// in `sessions_ended_before`).
	pub sessions_ended: usize,
	/// How many live sessions were ended before the change.
	pub sessions_ended_before: usize,
}

/// Why a User was not (fully) re-pointed.
#[derive(Debug, thiserror::Error)]
pub enum RepointError {
	/// A GitHub user ID is not a valid one.
	#[error("the GitHub user IDs must be positive numbers")]
	InvalidGithubUserId,
	/// Both IDs are the same.
	#[error("the new GitHub user ID is the one the User already has")]
	SameGithubUserId,
	/// No User has the current GitHub user ID.
	#[error("no User has the current GitHub user ID")]
	UnknownUser,
	/// Another User already has the new GitHub user ID (SR-03).
	#[error("another User already has the new GitHub user ID")]
	TargetInUse,
	/// The database refused the operation; nothing was changed.
	#[error("re-pointing failed and nothing was changed: {0}")]
	Storage(String),
	/// Sessions could not be ended before anything changed.
	#[error("sessions could not be ended and nothing was changed: {0}")]
	SessionsBeforeChange(String),
	/// The User was moved (and the move is recorded) but the sessions could not
	/// be ended afterwards. The command then deactivates the User so that no
	/// leftover session works.
	#[error("{}", sessions_after_change_message(.cause, .deactivation))]
	SessionsAfterChange {
		/// Why the sessions could not be ended.
		cause: String,
		/// What happened when the User was deactivated to close the gap.
		deactivation: Deactivation,
	},
}

/// The outcome of deactivating a User because their sessions could not be ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Deactivation {
	/// The User is deactivated: every leftover session is refused on its next
	/// request, because the session check reads `is_active` from the database.
	Done,
	/// Deactivating failed too (the reason is attached); leftover sessions may
	/// still work.
	Failed(String),
}

fn sessions_after_change_message(cause: &str, deactivation: &Deactivation) -> String {
	match deactivation {
		Deactivation::Done => format!(
			"the User was re-pointed, but their sessions could not be ended ({cause}). The account was deactivated so that every leftover session is refused on its next request; once Redis is reachable again, reactivate it by setting `is_active` on the User in the admin site"
		),
		Deactivation::Failed(reason) => format!(
			"the User was re-pointed, but their sessions could not be ended ({cause}) and the account could not be deactivated either ({reason}). Leftover sessions may still work: deactivate the User in the admin site now (clear `is_active`), and reactivate it after Redis is reachable"
		),
	}
}

impl From<OrmError> for RepointError {
	fn from(error: OrmError) -> Self {
		Self::Storage(error.to_string())
	}
}

/// Move the User with GitHub user ID `from` to GitHub user ID `to`.
///
/// Audited with the host operator as the actor. The success events are emitted
/// the moment the database change commits, before the sessions are ended a
/// second time, so the move is on record even if Redis then fails: two events
/// that share the User, `accounts.repoint.released` carrying the old numeric ID
/// and `accounts.repoint.claimed` carrying the new one (the shared audit shape
/// has one GitHub-ID field). A refusal emits `accounts.repoint.denied` and a
/// failure `accounts.repoint.failed`, each with a reason code.
///
/// If the sessions cannot be ended after the commit, the User is deactivated so
/// that the per-request check refuses every leftover session without relying on
/// Redis (fail closed): that emits `accounts.repoint.deactivated`, and a final
/// `accounts.repoint.failed` with reason `sessions_not_ended_after_change`.
///
/// # Errors
///
/// Returns [`RepointError`] when an ID is invalid, the User is unknown, the
/// target ID belongs to another User, or storage fails.
pub async fn repoint(
	from: i64,
	to: i64,
	sessions: &dyn SessionRevoker,
) -> Result<Repointed, RepointError> {
	let event = |name, outcome| AuditEvent::new(name, ActorKind::HostOperator, outcome);
	let refused = |error: RepointError| {
		let (name, outcome, reason, github_user) = match &error {
			RepointError::InvalidGithubUserId => (
				"accounts.repoint.denied",
				Outcome::Denied,
				"invalid_github_user_id",
				from,
			),
			RepointError::SameGithubUserId => (
				"accounts.repoint.denied",
				Outcome::Denied,
				"same_github_user_id",
				from,
			),
			RepointError::UnknownUser => (
				"accounts.repoint.denied",
				Outcome::Denied,
				"unknown_user",
				from,
			),
			RepointError::TargetInUse => (
				"accounts.repoint.denied",
				Outcome::Denied,
				"target_in_use",
				to,
			),
			RepointError::SessionsBeforeChange(_) => (
				"accounts.repoint.failed",
				Outcome::Failed,
				"sessions_not_ended_before_change",
				from,
			),
			// `SessionsAfterChange` is produced only after the commit and is
			// audited there; it never reaches this closure.
			RepointError::Storage(_) | RepointError::SessionsAfterChange { .. } => {
				("accounts.repoint.failed", Outcome::Failed, "storage", from)
			}
		};
		event(name, outcome)
			.github_user(github_user)
			.reason(reason)
			.emit();
		error
	};

	let user_id = match prepare(from, to, sessions).await {
		Ok(prepared) => prepared,
		Err(error) => return Err(refused(error)),
	};
	let sessions_ended_before = user_id.sessions_ended_before;
	let user_id = user_id.user_id;

	if let Err(error) = move_identity(user_id, from, to).await {
		return Err(refused(error));
	}
	// Committed. The move is on record before anything can fail again.
	event("accounts.repoint.released", Outcome::Succeeded)
		.subject_user(user_id)
		.github_user(from)
		.emit();
	event("accounts.repoint.claimed", Outcome::Succeeded)
		.subject_user(user_id)
		.github_user(to)
		.emit();

	match sessions.destroy_all_for_user(user_id).await {
		Ok(sessions_ended) => Ok(Repointed {
			user_id,
			sessions_ended,
			sessions_ended_before,
		}),
		Err(cause) => {
			let deactivation = match deactivate(user_id).await {
				Ok(()) => {
					event("accounts.repoint.deactivated", Outcome::Succeeded)
						.subject_user(user_id)
						.github_user(to)
						.reason("sessions_not_ended_after_change")
						.emit();
					Deactivation::Done
				}
				Err(reason) => Deactivation::Failed(reason),
			};
			event("accounts.repoint.failed", Outcome::Failed)
				.subject_user(user_id)
				.github_user(to)
				.reason("sessions_not_ended_after_change")
				.emit();
			Err(RepointError::SessionsAfterChange {
				cause: cause.to_string(),
				deactivation,
			})
		}
	}
}

/// What the checks and the first session pass established.
struct Prepared {
	user_id: Uuid,
	sessions_ended_before: usize,
}

/// Everything that must hold before the database changes: valid distinct IDs,
/// a User to move, a free target, and the sessions ended once.
async fn prepare(
	from: i64,
	to: i64,
	sessions: &dyn SessionRevoker,
) -> Result<Prepared, RepointError> {
	if from <= 0 || to <= 0 {
		return Err(RepointError::InvalidGithubUserId);
	}
	if from == to {
		return Err(RepointError::SameGithubUserId);
	}
	let user = find_by_github_user_id(from)
		.await
		.map_err(|error| RepointError::Storage(error.to_string()))?
		.ok_or(RepointError::UnknownUser)?;
	if find_by_github_user_id(to)
		.await
		.map_err(|error| RepointError::Storage(error.to_string()))?
		.is_some()
	{
		return Err(RepointError::TargetInUse);
	}

	let sessions_ended_before = sessions
		.destroy_all_for_user(user.id)
		.await
		.map_err(|error| RepointError::SessionsBeforeChange(error.to_string()))?;
	Ok(Prepared {
		user_id: user.id,
		sessions_ended_before,
	})
}

/// Deactivate the User so the session check refuses their leftover sessions.
async fn deactivate(user_id: Uuid) -> Result<(), String> {
	let updated = User::objects()
		.filter(User::field_id().eq(user_id))
		.update_fields([
			User::field_is_active().assign(false),
			User::field_updated_at().assign(persisted_now()),
		])
		.await
		.map_err(|error| error.to_string())?;
	if updated == 1 {
		Ok(())
	} else {
		Err("the User disappeared".to_owned())
	}
}

/// The database half, in one transaction: either the User has the new identity
/// and none of the old one's tokens or links, or nothing changed.
///
/// Losing the unique-constraint race to a concurrent sign-in or grant for the
/// target ID is the same refusal as finding it taken up front.
pub(crate) async fn move_identity(user_id: Uuid, from: i64, to: i64) -> Result<(), RepointError> {
	let placeholder = placeholder_login(to);
	let connection = get_connection().await?;
	let result: Result<(), OrmError> = connection
		.atomic(async |transaction| {
			let moved = User::objects()
				.filter(User::field_id().eq(user_id))
				.filter(User::field_github_user_id().eq(from))
				.update_fields_with_conn(
					transaction,
					[
						User::field_github_user_id().assign(to),
						User::field_github_login().assign(placeholder.clone()),
						User::field_display_name().assign(placeholder.clone()),
						User::field_avatar_url().assign(None::<String>),
						User::field_email().assign(None::<String>),
						User::field_updated_at().assign(persisted_now()),
					],
				)
				.await?;
			if moved != 1 {
				// Another operator changed the User since it was read. Roll back
				// rather than touch the tokens of an identity that is not `from`.
				return Err(OrmError::from(DatabaseError::new(
					DatabaseErrorKind::Transaction,
					"the User changed while it was being re-pointed".to_owned(),
				)));
			}
			SocialAccount::objects()
				.filter(SocialAccount::field_user_id().eq(user_id))
				.delete_with_conn(transaction)
				.await?;
			LoginLink::objects()
				.filter(LoginLink::field_user_id().eq(user_id))
				.filter(LoginLink::field_consumed_at().is_null())
				.delete_with_conn(transaction)
				.await?;
			Ok(())
		})
		.await;
	result.map_err(|error| {
		let taken = error
			.database_error()
			.is_some_and(|database| database.kind() == DatabaseErrorKind::UniqueViolation);
		if taken {
			RepointError::TargetInUse
		} else {
			RepointError::Storage(error.to_string())
		}
	})
}
