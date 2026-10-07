//! Project projections guarded by current organization membership.

use std::collections::HashMap;

use reinhardt::db::orm::Model;
use uuid::Uuid;

use super::models::{Environment, Project};
use super::services::{EnvironmentKind, EnvironmentSummary, ProjectDetail, ProjectSummary};
use crate::apps::deployment::read::latest_for_environment;
use crate::apps::organization::persistence::{OrganizationError, authorize_read};

pub async fn projects_for(
	user: Uuid,
	organization: Uuid,
) -> Result<Vec<ProjectSummary>, OrganizationError> {
	authorize_read(user, organization).await?;
	let projects = Project::objects()
		.filter(Project::field_organization_id().eq(organization))
		.order_by(&["display_name", "id"])
		.all()
		.await?;
	let environments = Environment::objects()
		.filter(Environment::field_organization_id().eq(organization))
		.all()
		.await?;
	let mut counts = HashMap::<Uuid, usize>::new();
	for environment in environments {
		*counts.entry(environment.project_id()).or_default() += 1;
	}
	Ok(projects
		.into_iter()
		.map(|project| ProjectSummary {
			id: project.id,
			organization_id: organization,
			name: project.display_name,
			repository: project.repository_url,
			environments: counts.get(&project.id).copied().unwrap_or(0),
		})
		.collect())
}

pub async fn project_detail(
	user: Uuid,
	organization: Uuid,
	project: Uuid,
) -> Result<ProjectDetail, OrganizationError> {
	authorize_read(user, organization).await?;
	let projects = Project::objects()
		.filter(Project::field_id().eq(project))
		.filter(Project::field_organization_id().eq(organization))
		.all()
		.await?;
	let project = projects
		.into_iter()
		.next()
		.ok_or(OrganizationError::Forbidden)?;
	let rows = Environment::objects()
		.filter(Environment::field_organization_id().eq(organization))
		.filter(Environment::field_project_id().eq(project.id))
		.order_by(&["kind", "id"])
		.all()
		.await?;
	let mut environments = Vec::with_capacity(rows.len());
	for environment in rows {
		let kind = EnvironmentKind::from_storage(&environment.kind).ok_or_else(|| {
			reinhardt::core::exception::Error::Internal("Unsupported Environment kind".into())
		})?;
		let desired_runtime = serde_json::from_str(&environment.desired_runtime).map_err(|_| {
			reinhardt::core::exception::Error::Internal("Unsupported runtime configuration".into())
		})?;
		let latest_operation = latest_for_environment(organization, environment.id).await?;
		environments.push(EnvironmentSummary {
			id: environment.id,
			kind,
			version: environment.version,
			desired_runtime,
			latest_operation,
		});
	}
	Ok(ProjectDetail {
		id: project.id,
		organization_id: organization,
		name: project.display_name,
		repository: project.repository_url,
		environments,
	})
}
