//! Project page routes and external read projections.
use super::persistence::{project_detail, projects_for};
use crate::apps::identity::persistence::Actor;
use crate::apps::organization::persistence::OrganizationError;
use crate::client::screens::DashboardState;
use reinhardt::{Request, Response, get};
#[get("/", name = "dashboard-index")]
pub async fn index(request: Request) -> reinhardt::http::Result<Response> {
	Ok(Response::new(http::StatusCode::SEE_OTHER).with_header(
		"Location",
		if request.extensions.get::<Actor>().is_some() {
			"/organizations/"
		} else {
			"/login/"
		},
	))
}
#[get("/organizations/{organization_id}/projects/", name = "projects-page")]
pub async fn projects_page(request: Request) -> reinhardt::http::Result<Response> {
	let Some(actor) = request.extensions.get::<Actor>() else {
		return Ok(Response::new(http::StatusCode::SEE_OTHER).with_header("Location", "/login/"));
	};
	let Some(organization) = request
		.path_params
		.get("organization_id")
		.and_then(|v| v.parse().ok())
	else {
		return Ok(Response::new(http::StatusCode::BAD_REQUEST));
	};
	let state = match projects_for(actor.id, organization).await {
		Ok(items) => DashboardState::Ready(items),
		Err(OrganizationError::Forbidden) => DashboardState::Forbidden,
		Err(_) => DashboardState::Unavailable,
	};
	crate::client::document::respond(request, state).await
}
#[get(
	"/organizations/{organization_id}/projects/{project_id}/",
	name = "project-page"
)]
pub async fn project_page(request: Request) -> reinhardt::http::Result<Response> {
	let Some(actor) = request.extensions.get::<Actor>() else {
		return Ok(Response::new(http::StatusCode::SEE_OTHER).with_header("Location", "/login/"));
	};
	let Some((organization, project)) = request
		.path_params
		.get("organization_id")
		.and_then(|v| v.parse().ok())
		.zip(
			request
				.path_params
				.get("project_id")
				.and_then(|v| v.parse().ok()),
		)
	else {
		return Ok(Response::new(http::StatusCode::BAD_REQUEST));
	};
	let state = match project_detail(actor.id, organization, project).await {
		Ok(detail) => DashboardState::Detail(Box::new(detail)),
		Err(OrganizationError::Forbidden) => DashboardState::Forbidden,
		Err(_) => DashboardState::Unavailable,
	};
	crate::client::document::respond(request, state).await
}
#[get("/api/v1/project/", name = "project-detail")]
pub async fn detail(request: Request) -> reinhardt::http::Result<Response> {
	let Some(actor) = request.extensions.get::<Actor>() else {
		return Ok(Response::new(http::StatusCode::UNAUTHORIZED));
	};
	#[derive(serde::Deserialize)]
	struct Query {
		organization: uuid::Uuid,
		project: uuid::Uuid,
	}
	let Ok(query) = request.query_as::<Query>() else {
		return Ok(Response::new(http::StatusCode::BAD_REQUEST));
	};
	match project_detail(actor.id, query.organization, query.project).await {
		Ok(detail) => Ok(Response::ok().with_json(&detail)?),
		Err(OrganizationError::Forbidden) => Ok(Response::new(http::StatusCode::FORBIDDEN)),
		Err(_) => Ok(Response::new(http::StatusCode::SERVICE_UNAVAILABLE)),
	}
}

#[get("/api/v1/projects/", name = "project-list")]
pub async fn list(request: Request) -> reinhardt::http::Result<Response> {
	let Some(actor) = request.extensions.get::<Actor>() else {
		return Ok(Response::new(http::StatusCode::UNAUTHORIZED));
	};
	#[derive(serde::Deserialize)]
	struct Query {
		organization: uuid::Uuid,
	}
	let Ok(query) = request.query_as::<Query>() else {
		return Ok(Response::new(http::StatusCode::BAD_REQUEST));
	};
	match projects_for(actor.id, query.organization).await {
		Ok(projects) => Ok(Response::ok()
			.with_header("Cache-Control", "no-store")
			.with_json(&projects)?),
		Err(OrganizationError::Forbidden) => Ok(Response::new(http::StatusCode::FORBIDDEN)),
		Err(_) => Ok(Response::new(http::StatusCode::SERVICE_UNAVAILABLE)),
	}
}
