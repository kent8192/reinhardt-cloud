//! The sign-up policy (SR-19).
//!
//! The policy decides whether an unknown GitHub identity may become a User. It
//! runs before any User, session, or stored token exists. An identity that
//! already has a User (including one pre-provisioned by `manage grant-staff`,
//! SR-105) never reaches it, so tightening the policy later cannot affect
//! existing Users.

use std::collections::BTreeSet;

use async_trait::async_trait;

/// The configured sign-up policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignUpPolicy {
	/// Any GitHub identity may sign up.
	Open,
	/// Only listed identities, and members of listed organizations, may sign up.
	Allowlist {
		/// Numeric GitHub user IDs admitted directly.
		user_ids: BTreeSet<i64>,
		/// Numeric GitHub organization IDs whose members are admitted.
		organization_ids: BTreeSet<i64>,
	},
	/// No unknown identity may sign up; only pre-provisioned Staff can sign in.
	InviteOnly,
}

/// A malformed allowlist entry.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SignUpPolicyError {
	/// An allowlist entry is not a positive decimal GitHub ID.
	#[error("sign-up allowlist entry {entry:?} is not a numeric GitHub ID")]
	InvalidAllowlistEntry {
		/// The offending entry.
		entry: String,
	},
}

impl SignUpPolicy {
	/// Build the policy from raw settings values.
	///
	/// An unrecognized or empty `name` resolves to [`SignUpPolicy::InviteOnly`]
	/// (SR-19). Allowlists are validated whichever policy is selected, so a
	/// malformed entry is reported even while the policy is `invite_only`.
	///
	/// # Errors
	///
	/// Returns [`SignUpPolicyError::InvalidAllowlistEntry`] for a malformed entry.
	pub fn from_settings(
		name: &str,
		user_ids: &str,
		organization_ids: &str,
	) -> Result<Self, SignUpPolicyError> {
		let user_ids = parse_id_list(user_ids)?;
		let organization_ids = parse_id_list(organization_ids)?;
		Ok(match name.trim().to_ascii_lowercase().as_str() {
			"open" => Self::Open,
			"allowlist" => Self::Allowlist {
				user_ids,
				organization_ids,
			},
			_ => Self::InviteOnly,
		})
	}

	/// Decide whether an unknown GitHub identity may sign up.
	///
	/// `membership` is consulted only under `allowlist`, only when the user ID
	/// is not listed directly, and only when organizations are listed. A failed
	/// lookup denies the sign-up rather than guessing.
	pub async fn decide(
		&self,
		github_user_id: i64,
		membership: &dyn OrganizationMembership,
	) -> SignUpDecision {
		match self {
			Self::Open => SignUpDecision::Admit(AdmitReason::Open),
			Self::InviteOnly => SignUpDecision::Deny(DenyReason::NotInvited),
			Self::Allowlist {
				user_ids,
				organization_ids,
			} => {
				if user_ids.contains(&github_user_id) {
					return SignUpDecision::Admit(AdmitReason::AllowlistedUser);
				}
				if organization_ids.is_empty() {
					return SignUpDecision::Deny(DenyReason::NotAllowlisted);
				}
				match membership.organization_ids(github_user_id).await {
					Ok(member_of) if member_of.iter().any(|id| organization_ids.contains(id)) => {
						SignUpDecision::Admit(AdmitReason::AllowlistedOrganization)
					}
					Ok(_) => SignUpDecision::Deny(DenyReason::NotAllowlisted),
					Err(MembershipError) => SignUpDecision::Deny(DenyReason::MembershipUnverified),
				}
			}
		}
	}
}

fn parse_id_list(raw: &str) -> Result<BTreeSet<i64>, SignUpPolicyError> {
	raw.split(',')
		.map(str::trim)
		.filter(|entry| !entry.is_empty())
		.map(|entry| match entry.parse::<i64>() {
			Ok(id) if id > 0 => Ok(id),
			_ => Err(SignUpPolicyError::InvalidAllowlistEntry {
				entry: entry.to_owned(),
			}),
		})
		.collect()
}

/// The outcome of a sign-up policy evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignUpDecision {
	/// The identity may become a User.
	Admit(AdmitReason),
	/// The identity may not become a User.
	Deny(DenyReason),
}

/// Why an identity was admitted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmitReason {
	/// The policy is `open`.
	Open,
	/// The numeric user ID is on the allowlist.
	AllowlistedUser,
	/// The identity belongs to an allowlisted organization.
	AllowlistedOrganization,
}

/// Why an identity was denied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DenyReason {
	/// The policy admits only invited identities and none is pre-provisioned.
	NotInvited,
	/// The identity is neither listed nor in a listed organization.
	NotAllowlisted,
	/// Organization membership could not be verified against GitHub.
	MembershipUnverified,
}

impl AdmitReason {
	/// Short code recorded in the audit log.
	#[must_use]
	pub const fn code(self) -> &'static str {
		match self {
			Self::Open => "policy_open",
			Self::AllowlistedUser => "allowlisted_user",
			Self::AllowlistedOrganization => "allowlisted_organization",
		}
	}
}

impl DenyReason {
	/// Short code recorded in the audit log. It is never shown to the identity
	/// that was denied, so the policy contents stay private.
	#[must_use]
	pub const fn code(self) -> &'static str {
		match self {
			Self::NotInvited => "not_invited",
			Self::NotAllowlisted => "not_allowlisted",
			Self::MembershipUnverified => "membership_unverified",
		}
	}
}

/// Resolves a GitHub user's organization memberships on the server.
///
/// The implementation calls GitHub with the credentials of the signing-in user;
/// the answer must never come from the browser. It returns numeric
/// organization IDs, never logins.
#[async_trait]
pub trait OrganizationMembership: Send + Sync {
	/// Numeric IDs of the organizations the user belongs to.
	///
	/// # Errors
	///
	/// Returns [`MembershipError`] when GitHub cannot answer.
	async fn organization_ids(&self, github_user_id: i64) -> Result<Vec<i64>, MembershipError>;
}

/// GitHub could not confirm organization membership.
///
/// Carries no detail on purpose: provider error text never reaches a client or
/// an audit field.
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
#[error("organization membership could not be verified")]
pub struct MembershipError;
