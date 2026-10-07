//! Current membership checks and personal-organization creation.

use reinhardt::db::orm::{Model, transaction::AtomicTransaction};
use uuid::Uuid;

use super::models::{Membership, Organization};
use super::services::{
	Action, AuthorizationService, Membership as MembershipFacts, OrganizationSummary, Role,
};

#[derive(Debug, thiserror::Error)]
pub enum OrganizationError {
	#[error("Organization access is forbidden")]
	Forbidden,
	#[error("Organization storage is unavailable")]
	Storage(#[from] reinhardt::core::exception::Error),
}

pub async fn authorize_read(user: Uuid, organization: Uuid) -> Result<(), OrganizationError> {
	let members = Membership::objects()
		.filter(Membership::field_user_id().eq(user))
		.filter(Membership::field_organization_id().eq(organization))
		.limit(1)
		.all()
		.await?;
	let member = members.first().ok_or(OrganizationError::Forbidden)?;
	let role = Role::from_storage(&member.role).ok_or(OrganizationError::Forbidden)?;
	AuthorizationService::new(
		&[MembershipFacts {
			user_id: user,
			organization_id: organization,
			role,
		}],
		&[],
	)
	.authorize(user, organization, None, Action::ReadApplication)
	.map_err(|_| OrganizationError::Forbidden)
}

pub async fn organizations_for(user: Uuid) -> Result<Vec<OrganizationSummary>, OrganizationError> {
	let memberships = Membership::objects()
		.filter(Membership::field_user_id().eq(user))
		.all()
		.await?;
	let mut result = Vec::with_capacity(memberships.len());
	for membership in memberships {
		if Role::from_storage(&membership.role).is_none() {
			continue;
		}
		let organization = Organization::objects()
			.get(membership.organization_id())
			.get()
			.await?;
		result.push(OrganizationSummary {
			id: organization.id,
			name: organization.display_name,
		});
	}
	result.sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
	Ok(result)
}

/// Participate in the account transaction without publishing a partial membership.
pub async fn create_personal_organization(
	transaction: &mut AtomicTransaction,
	user: Uuid,
	name: &str,
) -> reinhardt::core::exception::Result<Uuid> {
	let organization = Organization::new().display_name(name).finish();
	let organization = Organization::objects()
		.create_with_conn(transaction, &organization)
		.await?;
	let membership = Membership::new()
		.organization(organization.id)
		.user(user)
		.role("owner")
		.finish();
	Membership::objects()
		.create_with_conn(transaction, &membership)
		.await?;
	Ok(organization.id)
}
