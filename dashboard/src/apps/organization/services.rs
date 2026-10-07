//! Organization authorization shared by every transport.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::apps::project::services::EnvironmentKind;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrganizationSummary {
	pub id: Uuid,
	pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
	Owner,
	Admin,
	Developer,
	Viewer,
}

impl Role {
	pub fn from_storage(value: &str) -> Option<Self> {
		match value {
			"owner" => Some(Self::Owner),
			"admin" => Some(Self::Admin),
			"developer" => Some(Self::Developer),
			"viewer" => Some(Self::Viewer),
			_ => None,
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
	ReadApplication,
	ChangeApplication,
	ApproveConfiguration,
	ChangeSecret,
	DeleteResource,
	ManageMembers,
	ReadAudit,
	ManageOwnership,
}

/// Current membership facts, reloaded at authorization boundaries.
#[derive(Debug, Clone, Copy)]
pub struct Membership {
	pub user_id: Uuid,
	pub organization_id: Uuid,
	pub role: Role,
}

/// An execution grant does not grant administrative production permissions.
#[derive(Debug, Clone, Copy)]
pub struct EnvironmentGrant {
	pub user_id: Uuid,
	pub organization_id: Uuid,
	pub environment_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationError {
	NotAMember,
	Forbidden,
	LastOwner,
}

pub struct AuthorizationService<'a> {
	memberships: &'a [Membership],
	grants: &'a [EnvironmentGrant],
}

impl<'a> AuthorizationService<'a> {
	pub fn new(memberships: &'a [Membership], grants: &'a [EnvironmentGrant]) -> Self {
		Self {
			memberships,
			grants,
		}
	}

	pub fn authorize(
		&self,
		user_id: Uuid,
		organization_id: Uuid,
		environment: Option<(Uuid, EnvironmentKind)>,
		action: Action,
	) -> Result<(), AuthorizationError> {
		let membership = self
			.memberships
			.iter()
			.find(|membership| {
				membership.user_id == user_id && membership.organization_id == organization_id
			})
			.ok_or(AuthorizationError::NotAMember)?;
		let allowed = match (membership.role, action) {
			(Role::Owner, _) => true,
			(Role::Admin, Action::ManageOwnership) => false,
			(Role::Admin, _) => true,
			(_, Action::ReadApplication) => true,
			(Role::Developer, Action::ChangeApplication) => {
				environment.is_some_and(|(id, kind)| {
					kind != EnvironmentKind::Production
						|| self.grants.iter().any(|grant| {
							grant.user_id == user_id
								&& grant.organization_id == organization_id
								&& grant.environment_id == id
						})
				})
			}
			(Role::Developer, Action::ApproveConfiguration | Action::ChangeSecret) => {
				environment.is_some_and(|(_, kind)| kind != EnvironmentKind::Production)
			}
			_ => false,
		};
		allowed.then_some(()).ok_or(AuthorizationError::Forbidden)
	}

	pub fn ensure_owner_can_leave(
		&self,
		user_id: Uuid,
		organization_id: Uuid,
	) -> Result<(), AuthorizationError> {
		let owners = self
			.memberships
			.iter()
			.filter(|member| {
				member.organization_id == organization_id && member.role == Role::Owner
			})
			.collect::<Vec<_>>();
		if owners.len() == 1 && owners[0].user_id == user_id {
			return Err(AuthorizationError::LastOwner);
		}
		Ok(())
	}
}
