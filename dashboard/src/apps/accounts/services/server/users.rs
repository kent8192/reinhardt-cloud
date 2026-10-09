//! User lookup, first sign-in, and profile synchronization.
//!
//! The numeric GitHub user ID is the only key used to find a User (SR-02). The
//! login, name, avatar, and email arriving from GitHub are profile data that is
//! copied onto the existing User; they never select, merge, or create one.

use reinhardt::db::orm::Model;

use crate::audit::{ActorKind, AuditEvent, Outcome};

use crate::apps::accounts::models::User;
use crate::apps::accounts::services::server::sign_up_policy::{
	AdmitReason, DenyReason, OrganizationMembership, SignUpDecision, SignUpPolicy,
};

const MAX_LOGIN_LEN: usize = 64;
const MAX_DISPLAY_NAME_LEN: usize = 255;
const MAX_AVATAR_URL_LEN: usize = 2048;
const MAX_EMAIL_LEN: usize = 254;

/// Profile data read from GitHub for one signing-in account.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GithubProfile {
	/// Numeric GitHub user ID.
	pub github_user_id: i64,
	/// Current GitHub login.
	pub login: String,
	/// Name set on the GitHub profile, if any.
	pub name: Option<String>,
	/// Avatar URL, if any.
	pub avatar_url: Option<String>,
	/// Verified primary email, if one is known.
	pub verified_email: Option<String>,
}

impl GithubProfile {
	fn validate(&self) -> Result<(), UserError> {
		let valid = self.github_user_id > 0
			&& !self.login.is_empty()
			&& self.login.len() <= MAX_LOGIN_LEN
			&& self
				.name
				.as_deref()
				.is_none_or(|v| v.len() <= MAX_DISPLAY_NAME_LEN)
			&& self
				.avatar_url
				.as_deref()
				.is_none_or(|v| v.len() <= MAX_AVATAR_URL_LEN)
			&& self
				.verified_email
				.as_deref()
				.is_none_or(|v| v.len() <= MAX_EMAIL_LEN);
		if valid {
			Ok(())
		} else {
			Err(UserError::InvalidProfile)
		}
	}

	/// The name shown in the Dashboard: the profile name, or the login.
	fn display_name(&self) -> &str {
		self.name
			.as_deref()
			.map(str::trim)
			.filter(|name| !name.is_empty())
			.unwrap_or(&self.login)
	}
}

/// The User that a first sign-in resolved to.
#[derive(Clone, Debug)]
pub enum ResolvedUser {
	/// The GitHub identity already had a User (including one pre-provisioned
	/// by `manage grant-staff`). The sign-up policy was not consulted.
	Existing(User),
	/// The policy admitted the identity and a User was created.
	Created(User),
}

impl ResolvedUser {
	/// The resolved User.
	#[must_use]
	pub fn user(&self) -> &User {
		match self {
			Self::Existing(user) | Self::Created(user) => user,
		}
	}

	/// Consume the outcome and return the User.
	#[must_use]
	pub fn into_user(self) -> User {
		match self {
			Self::Existing(user) | Self::Created(user) => user,
		}
	}
}

/// Why a first sign-in produced no User.
#[derive(Debug, thiserror::Error)]
pub enum FirstSignInError {
	/// The sign-up policy refused the identity. Nothing was persisted.
	#[error("sign-up was denied by policy")]
	Denied(DenyReason),
	/// The profile or the database failed.
	#[error(transparent)]
	User(#[from] UserError),
}

/// Failures of the user service.
#[derive(Debug, thiserror::Error)]
pub enum UserError {
	/// The GitHub profile is not usable (non-positive ID, empty login, or a
	/// field longer than its column).
	#[error("GitHub profile is not valid")]
	InvalidProfile,
	/// The database refused the operation.
	#[error("user storage failed: {0}")]
	Storage(String),
}

impl UserError {
	fn storage(error: impl std::fmt::Display) -> Self {
		Self::Storage(error.to_string())
	}
}

/// Find the User for a numeric GitHub user ID.
///
/// # Errors
///
/// Returns [`UserError::Storage`] when the query fails.
pub async fn find_by_github_user_id(github_user_id: i64) -> Result<Option<User>, UserError> {
	User::objects()
		.filter(User::field_github_user_id().eq(github_user_id))
		.first()
		.await
		.map_err(UserError::storage)
}

/// Resolve the User for a signing-in GitHub identity, creating one only when
/// the sign-up policy admits it (SR-19).
///
/// An identity that already has a User is returned as-is and is not affected
/// by the policy. For an unknown identity the policy runs first; a refusal
/// leaves no User, no token, and no other persistent trace, only an audit
/// event. Concurrent first sign-ins for one identity create exactly one User
/// (SR-03): the loser of the race returns the winner's User.
///
/// # Errors
///
/// Returns [`FirstSignInError::Denied`] when the policy refuses the identity,
/// and [`FirstSignInError::User`] for an invalid profile or a storage failure.
pub async fn resolve_first_sign_in(
	profile: &GithubProfile,
	policy: &SignUpPolicy,
	membership: &dyn OrganizationMembership,
) -> Result<ResolvedUser, FirstSignInError> {
	profile.validate()?;
	if let Some(user) = find_by_github_user_id(profile.github_user_id).await? {
		return Ok(ResolvedUser::Existing(user));
	}

	match policy.decide(profile.github_user_id, membership).await {
		SignUpDecision::Deny(reason) => {
			AuditEvent::new("accounts.sign_up.denied", ActorKind::User, Outcome::Denied)
				.github_user(profile.github_user_id)
				.reason(reason.code())
				.emit();
			Err(FirstSignInError::Denied(reason))
		}
		SignUpDecision::Admit(reason) => create_user(profile, reason).await,
	}
}

async fn create_user(
	profile: &GithubProfile,
	reason: AdmitReason,
) -> Result<ResolvedUser, FirstSignInError> {
	let new_user = User::build()
		.github_user_id(profile.github_user_id)
		.github_login(profile.login.clone())
		.display_name(profile.display_name().to_owned())
		.avatar_url(profile.avatar_url.clone())
		.email(profile.verified_email.clone())
		.is_active(true)
		.is_staff(false)
		.finish();

	match User::objects().create(&new_user).await {
		Ok(user) => {
			AuditEvent::new(
				"accounts.sign_up.admitted",
				ActorKind::User,
				Outcome::Succeeded,
			)
			.subject_user(user.id)
			.github_user(user.github_user_id)
			.reason(reason.code())
			.emit();
			Ok(ResolvedUser::Created(user))
		}
		Err(create_error) => {
			// A concurrent first sign-in for the same GitHub ID may have won the
			// unique constraint; its User is the one User this identity maps to.
			match find_by_github_user_id(profile.github_user_id).await? {
				Some(user) => Ok(ResolvedUser::Existing(user)),
				None => Err(UserError::storage(create_error).into()),
			}
		}
	}
}

/// Copy the GitHub profile onto an existing User.
///
/// Identity, activation, and Staff status are never touched: a renamed GitHub
/// login updates the displayed data of the same User (SR-02). The row is
/// written only when something changed.
///
/// # Errors
///
/// Returns [`UserError::InvalidProfile`] when the profile is unusable or does
/// not belong to `user`, and [`UserError::Storage`] when the update fails.
pub async fn sync_profile(user: User, profile: &GithubProfile) -> Result<User, UserError> {
	profile.validate()?;
	if user.github_user_id != profile.github_user_id {
		return Err(UserError::InvalidProfile);
	}

	let display_name = profile.display_name();
	let unchanged = user.github_login == profile.login
		&& user.display_name == display_name
		&& user.avatar_url == profile.avatar_url
		&& user.email == profile.verified_email;
	if unchanged {
		return Ok(user);
	}

	// Partial update: the in-memory copy may be stale with respect to Staff and
	// activation changes made by `manage`, so only profile columns are written.
	let updated = User::objects()
		.filter(User::field_id().eq(user.id))
		.update_fields([
			User::field_github_login().assign(profile.login.clone()),
			User::field_display_name().assign(display_name.to_owned()),
			User::field_avatar_url().assign(profile.avatar_url.clone()),
			User::field_email().assign(profile.verified_email.clone()),
			User::field_updated_at().assign(chrono::Utc::now()),
		])
		.await
		.map_err(UserError::storage)?;
	if updated != 1 {
		return Err(UserError::Storage(
			"user disappeared during profile sync".to_owned(),
		));
	}
	find_by_github_user_id(profile.github_user_id)
		.await?
		.ok_or_else(|| UserError::Storage("user disappeared during profile sync".to_owned()))
}
