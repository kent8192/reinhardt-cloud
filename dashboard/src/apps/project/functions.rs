//! Organization-scoped typed Project queries for Pages.
use super::services::{ProjectDetail, ProjectSummary};
#[cfg(server)]
use reinhardt::pages::server_fn::ServerFnRequest;
use reinhardt::pages::server_fn::{ServerFnError, server_fn};
use uuid::Uuid;
#[server_fn(endpoint = "/api/server_fn/projects/")]
pub async fn load_projects(
	organization_id: Uuid,
	#[inject] request: ServerFnRequest,
) -> Result<Vec<ProjectSummary>, ServerFnError> {
	let actor = crate::apps::identity::functions::actor(&request)?;
	super::persistence::projects_for(actor, organization_id)
		.await
		.map_err(crate::apps::identity::functions::read_error)
}
#[server_fn(endpoint = "/api/server_fn/project/")]
pub async fn load_project(
	organization_id: Uuid,
	project_id: Uuid,
	#[inject] request: ServerFnRequest,
) -> Result<ProjectDetail, ServerFnError> {
	let actor = crate::apps::identity::functions::actor(&request)?;
	super::persistence::project_detail(actor, organization_id, project_id)
		.await
		.map_err(crate::apps::identity::functions::read_error)
}
