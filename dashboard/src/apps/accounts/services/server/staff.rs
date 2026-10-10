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
use crate::apps::accounts::services::server::sessions::SessionRevoker;
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
	/// The User was not Staff; nothing changed and no session was touched, just
	/// as a repeated grant changes nothing.
	NotStaff {
		/// The User.
		user_id: Uuid,
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
	/// The revocation committed but the User's sessions could not be ended.
	///
	/// Leftover sessions are harmless to the revocation: the Staff flag is read
	/// from the database on every request.
	#[error("{}", sessions_not_ended_message(.cause, *.removed))]
	SessionsNotEnded {
		/// Why the sessions could not be ended.
		cause: String,
		/// Whether the User row was removed (a never-used pre-provisioned User).
		removed: bool,
	},
}

fn sessions_not_ended_message(cause: &str, removed: bool) -> String {
	if removed {
		format!(
			"the pre-provisioned User was removed, but their sessions could not be ended ({cause}). Any leftover session is refused: the User no longer exists"
		)
	} else {
		format!(
			"Staff was revoked, but the User's sessions could not be ended ({cause}). Staff powers have already stopped: the Staff flag is read from the database on every request, so a leftover session has ordinary-User access only and expires on its own (30 minutes idle, 24 hours at most)"
		)
	}
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

/// What a revocation changed in the database, before the sessions are touched.
enum Committed {
	/// The User stopped being Staff.
	Demoted(Uuid),
	/// The never-used pre-provisioned User was removed.
	Removed(Uuid),
	/// The User was not Staff; nothing changed.
	NotStaff(Uuid),
}

/// Remove Staff from the GitHub account `github_user_id` and end its sessions.
///
/// Revoking a User who is not Staff changes nothing and ends no session: the
/// outcome is `unchanged`, as for a repeated grant.
///
/// A User that was pre-provisioned and has never signed in is removed instead:
/// leaving the row would keep the policy exemption of SR-105 alive after the
/// operator withdrew the grant. Audited as `accounts.revoke_staff.succeeded` /
/// `.denied` / `.failed`.
///
/// The success event is emitted the moment the database change commits, before
/// the sessions are ended, so a Redis failure cannot lose the record of the
/// revocation (SR-20). If ending the sessions then fails, an additional
/// `accounts.revoke_staff.failed` with reason `sessions_not_ended` follows. No
/// session fallback is needed to fail closed: the Staff flag is read from the
/// database on every request (`SessionAuthMiddleware`, `AccessGate`), so a
/// leftover session of a demoted User has ordinary access only and one of a
/// removed User is anonymous.
///
/// # Errors
///
/// Returns [`StaffError::UnknownUser`] when no User has the ID,
/// [`StaffError::Storage`] when the database fails, and
/// [`StaffError::SessionsNotEnded`] when Staff was cleared but Redis refused to
/// end the sessions.
pub async fn revoke(
	github_user_id: i64,
	sessions: &dyn SessionRevoker,
) -> Result<RevokeOutcome, StaffError> {
	let event = |name, outcome| {
		AuditEvent::new(name, ActorKind::HostOperator, outcome).github_user(github_user_id)
	};
	let committed = match change(github_user_id).await {
		Ok(committed) => committed,
		Err(error) => {
			match &error {
				StaffError::InvalidGithubUserId => {
					event("accounts.revoke_staff.denied", Outcome::Denied)
						.reason("invalid_github_user_id")
						.emit();
				}
				StaffError::UnknownUser => event("accounts.revoke_staff.denied", Outcome::Denied)
					.reason("unknown_user")
					.emit(),
				StaffError::Storage(_) | StaffError::SessionsNotEnded { .. } => {
					event("accounts.revoke_staff.failed", Outcome::Failed)
						.reason("storage")
						.emit();
				}
			}
			return Err(error);
		}
	};

	let (user_id, reason, removed) = match committed {
		Committed::Demoted(id) => (id, "revoked", false),
		Committed::Removed(id) => (id, "pre_provisioned_removed", true),
		Committed::NotStaff(id) => (id, "unchanged", false),
	};
	event("accounts.revoke_staff.succeeded", Outcome::Succeeded)
		.subject_user(user_id)
		.reason(reason)
		.emit();
	if let Committed::NotStaff(user_id) = committed {
		// Idempotent, like a repeated grant: no change, so no session is ended.
		return Ok(RevokeOutcome::NotStaff { user_id });
	}

	match sessions.destroy_all_for_user(user_id).await {
		Ok(sessions_ended) => Ok(match committed {
			Committed::Demoted(_) => RevokeOutcome::Revoked {
				user_id,
				sessions_ended,
			},
			Committed::Removed(_) => RevokeOutcome::PreProvisionRemoved { user_id },
			Committed::NotStaff(_) => RevokeOutcome::NotStaff { user_id },
		}),
		Err(cause) => {
			event("accounts.revoke_staff.failed", Outcome::Failed)
				.subject_user(user_id)
				.reason("sessions_not_ended")
				.emit();
			Err(StaffError::SessionsNotEnded {
				cause: cause.to_string(),
				removed,
			})
		}
	}
}

/// The database half of a revocation.
async fn change(github_user_id: i64) -> Result<Committed, StaffError> {
	if github_user_id <= 0 {
		return Err(StaffError::InvalidGithubUserId);
	}
	let user = find_by_github_user_id(github_user_id)
		.await?
		.ok_or(StaffError::UnknownUser)?;

	if !user.is_staff {
		return Ok(Committed::NotStaff(user.id));
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
		return Ok(Committed::Removed(user.id));
	}

	User::objects()
		.filter(User::field_id().eq(user.id))
		.update_fields([
			User::field_is_staff().assign(false),
			User::field_updated_at().assign(persisted_now()),
		])
		.await?;
	Ok(Committed::Demoted(user.id))
}
