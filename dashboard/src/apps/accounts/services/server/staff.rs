//! Staff grants and revocations (SR-20) and the pre-provisioning of the first
//! User of a deployment (SR-105).
//!
//! Both operations are reachable only from `manage grant-staff`; no HTTP,
//! WebSocket, or gRPC endpoint, Invitation, CLI Session, or Login Link calls
//! them. A grant for a GitHub account that has never signed in creates the User
//! row, keyed only by the numeric GitHub user ID. That row is what lets the
//! account sign in whatever the sign-up policy says: the first GitHub sign-in
//! finds an existing User and never consults the policy
//! (`users::resolve_first_sign_in`), and the exemption belongs to that one row,
//! not to a list other accounts could join.

use reinhardt::core::exception::Error as OrmError;
use reinhardt::db::orm::{Model, get_connection};
use uuid::Uuid;

use crate::apps::accounts::models::User;
use crate::apps::accounts::services::server::sessions::SessionService;
use crate::apps::accounts::services::server::users::{UserError, find_by_github_user_id};
use crate::audit::{ActorKind, AuditEvent, Outcome};
use crate::persisted_time::persisted_now;

/// How a grant ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantOutcome {
	/// No User had this GitHub user ID; one was created, as Staff, and can sign
	/// in whatever the sign-up policy is.
	PreProvisioned {
		/// The new User.
		user_id: Uuid,
	},
	/// An existing User became Staff.
	Granted {
		/// The User.
		user_id: Uuid,
		/// Whether the User is deactivated and so cannot sign in despite the grant.
		deactivated: bool,
	},
	/// The User already was Staff; nothing changed.
	AlreadyStaff {
		/// The User.
		user_id: Uuid,
	},
}

/// How a revocation ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevokeOutcome {
	/// The User stopped being Staff and `sessions_ended` sessions were ended.
	Revoked {
		/// The User.
		user_id: Uuid,
		/// How many live sessions were destroyed.
		sessions_ended: usize,
	},
	/// The User was pre-provisioned and never signed in. The row was removed, so
	/// the grant's sign-up exemption ends with it.
	PreProvisionRemoved {
		/// The removed User.
		user_id: Uuid,
	},
	/// The User was not Staff; their sessions were ended anyway.
	NotStaff {
		/// The User.
		user_id: Uuid,
		/// How many live sessions were destroyed.
		sessions_ended: usize,
	},
}

/// Why a Staff change was not made.
#[derive(Debug, thiserror::Error)]
pub enum StaffError {
	/// The numeric GitHub user ID is not a valid one.
	#[error("the GitHub user ID must be a positive number")]
	InvalidGithubUserId,
	/// A revocation named a GitHub user ID that no User has.
	#[error("no User has this GitHub user ID")]
	UnknownUser,
	/// The database refused the operation.
	#[error("staff storage failed: {0}")]
	Storage(String),
	/// The Staff flag was cleared but the User's sessions could not be ended.
	#[error("Staff was revoked but the sessions could not be ended: {0}")]
	SessionsNotEnded(String),
}

impl From<UserError> for StaffError {
	fn from(error: UserError) -> Self {
		Self::Storage(error.to_string())
	}
}

impl From<OrmError> for StaffError {
	fn from(error: OrmError) -> Self {
		Self::Storage(error.to_string())
	}
}

/// The login and display name given to a User pre-provisioned by `grant-staff`
/// (and to a User re-pointed to a new GitHub account) before GitHub has
/// reported their profile. It is the numeric ID in text form: it names nobody
/// else's login. The first sign-in replaces it (`users::sync_profile`).
#[must_use]
pub fn placeholder_login(github_user_id: i64) -> String {
	format!("github-{github_user_id}")
}

/// Make the GitHub account `github_user_id` Staff, creating its User when it
/// has none.
///
/// Audited as `accounts.grant_staff.succeeded` / `.denied` / `.failed` with the
/// host operator as the actor and the numeric GitHub user ID.
///
/// # Errors
///
/// Returns [`StaffError::InvalidGithubUserId`] for a non-positive ID and
/// [`StaffError::Storage`] when the database fails.
pub async fn grant(github_user_id: i64) -> Result<GrantOutcome, StaffError> {
	let result = grant_inner(github_user_id).await;
	let event = |name, outcome| {
		AuditEvent::new(name, ActorKind::HostOperator, outcome).github_user(github_user_id)
	};
	match &result {
		Ok(outcome) => {
			let (user_id, reason) = match outcome {
				GrantOutcome::PreProvisioned { user_id } => (*user_id, "pre_provisioned"),
				GrantOutcome::Granted { user_id, .. } => (*user_id, "existing_user"),
				GrantOutcome::AlreadyStaff { user_id } => (*user_id, "unchanged"),
			};
			event("accounts.grant_staff.succeeded", Outcome::Succeeded)
				.subject_user(user_id)
				.reason(reason)
				.emit();
		}
		Err(StaffError::InvalidGithubUserId) => {
			event("accounts.grant_staff.denied", Outcome::Denied)
				.reason("invalid_github_user_id")
				.emit()
		}
		Err(_) => event("accounts.grant_staff.failed", Outcome::Failed)
			.reason("storage")
			.emit(),
	}
	result
}

async fn grant_inner(github_user_id: i64) -> Result<GrantOutcome, StaffError> {
	if github_user_id <= 0 {
		return Err(StaffError::InvalidGithubUserId);
	}
	if let Some(user) = find_by_github_user_id(github_user_id).await? {
		return make_staff(user).await;
	}

	let placeholder = placeholder_login(github_user_id);
	let new_user = User::build()
		.github_user_id(github_user_id)
		.github_login(placeholder.clone())
		.display_name(placeholder)
		.avatar_url(None)
		.email(None)
		.is_active(true)
		.is_staff(true)
		.last_login(None)
		.finish();
	match User::objects().create(&new_user).await {
		Ok(user) => Ok(GrantOutcome::PreProvisioned { user_id: user.id }),
		Err(create_error) => {
			// A concurrent sign-in or grant for the same GitHub ID may have won the
			// unique constraint; its User is the one User this identity maps to.
			match find_by_github_user_id(github_user_id).await? {
				Some(user) => make_staff(user).await,
				None => Err(create_error.into()),
			}
		}
	}
}

async fn make_staff(user: User) -> Result<GrantOutcome, StaffError> {
	if user.is_staff {
		return Ok(GrantOutcome::AlreadyStaff { user_id: user.id });
	}
	User::objects()
		.filter(User::field_id().eq(user.id))
		.update_fields([
			User::field_is_staff().assign(true),
			User::field_updated_at().assign(persisted_now()),
		])
		.await?;
	Ok(GrantOutcome::Granted {
		user_id: user.id,
		deactivated: !user.is_active,
	})
}

/// Remove Staff from the GitHub account `github_user_id` and end its sessions.
///
/// A User that was pre-provisioned and has never signed in is removed instead:
/// leaving the row would keep the policy exemption of SR-105 alive after the
/// operator withdrew the grant. Audited as `accounts.revoke_staff.succeeded` /
/// `.denied` / `.failed`.
///
/// # Errors
///
/// Returns [`StaffError::UnknownUser`] when no User has the ID,
/// [`StaffError::Storage`] when the database fails, and
/// [`StaffError::SessionsNotEnded`] when Staff was cleared but Redis refused to
/// end the sessions.
pub async fn revoke(
	github_user_id: i64,
	sessions: &SessionService,
) -> Result<RevokeOutcome, StaffError> {
	let result = revoke_inner(github_user_id, sessions).await;
	let event = |name, outcome| {
		AuditEvent::new(name, ActorKind::HostOperator, outcome).github_user(github_user_id)
	};
	match &result {
		Ok(outcome) => {
			let (user_id, reason) = match outcome {
				RevokeOutcome::Revoked { user_id, .. } => (*user_id, "revoked"),
				RevokeOutcome::PreProvisionRemoved { user_id } => {
					(*user_id, "pre_provisioned_removed")
				}
				RevokeOutcome::NotStaff { user_id, .. } => (*user_id, "unchanged"),
			};
			event("accounts.revoke_staff.succeeded", Outcome::Succeeded)
				.subject_user(user_id)
				.reason(reason)
				.emit();
		}
		Err(StaffError::InvalidGithubUserId) => {
			event("accounts.revoke_staff.denied", Outcome::Denied)
				.reason("invalid_github_user_id")
				.emit();
		}
		Err(StaffError::UnknownUser) => event("accounts.revoke_staff.denied", Outcome::Denied)
			.reason("unknown_user")
			.emit(),
		Err(StaffError::SessionsNotEnded(_)) => {
			event("accounts.revoke_staff.failed", Outcome::Failed)
				.reason("sessions_not_ended")
				.emit();
		}
		Err(StaffError::Storage(_)) => event("accounts.revoke_staff.failed", Outcome::Failed)
			.reason("storage")
			.emit(),
	}
	result
}

async fn revoke_inner(
	github_user_id: i64,
	sessions: &SessionService,
) -> Result<RevokeOutcome, StaffError> {
	if github_user_id <= 0 {
		return Err(StaffError::InvalidGithubUserId);
	}
	let user = find_by_github_user_id(github_user_id)
		.await?
		.ok_or(StaffError::UnknownUser)?;

	if !user.is_staff {
		let sessions_ended = end_sessions(sessions, user.id).await?;
		return Ok(RevokeOutcome::NotStaff {
			user_id: user.id,
			sessions_ended,
		});
	}

	// One conditional delete decides "pre-provisioned and never used": a User who
	// has signed in has a `last_login` and is only demoted.
	let mut connection = get_connection().await?;
	let removed = User::objects()
		.filter(User::field_id().eq(user.id))
		.filter(User::field_is_staff().eq(true))
		.filter(User::field_last_login().is_null())
		.delete_with_conn(&mut connection)
		.await?;
	if removed == 1 {
		// Nothing can hold a session for a User who never signed in, but a
		// session created in the instant before the delete must not outlive it.
		end_sessions(sessions, user.id).await?;
		return Ok(RevokeOutcome::PreProvisionRemoved { user_id: user.id });
	}

	User::objects()
		.filter(User::field_id().eq(user.id))
		.update_fields([
			User::field_is_staff().assign(false),
			User::field_updated_at().assign(persisted_now()),
		])
		.await?;
	let sessions_ended = end_sessions(sessions, user.id).await?;
	Ok(RevokeOutcome::Revoked {
		user_id: user.id,
		sessions_ended,
	})
}

async fn end_sessions(sessions: &SessionService, user_id: Uuid) -> Result<usize, StaffError> {
	sessions
		.destroy_all_for_user(user_id)
		.await
		.map_err(|error| StaffError::SessionsNotEnded(error.to_string()))
}
