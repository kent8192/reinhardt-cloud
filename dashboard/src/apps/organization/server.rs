//! Organization read transport; models stay inside the App.

use reinhardt::{Request, Response, get};

use super::persistence::organizations_for;
use crate::apps::identity::persistence::Actor;

#[get("/api/v1/organizations/", name = "organizations")]
pub async fn list(request: Request) -> reinhardt::http::Result<Response> {
	let Some(actor) = request.extensions.get::<Actor>() else {
		return Ok(Response::new(http::StatusCode::UNAUTHORIZED));
	};
	match organizations_for(actor.id).await {
		Ok(organizations) => Ok(Response::ok()
			.with_header("Cache-Control", "no-store")
			.with_json(&organizations)?),
		Err(_) => Ok(Response::new(http::StatusCode::SERVICE_UNAVAILABLE)),
	}
}

#[reinhardt::get("/organizations/", name = "organization-chooser-page")]
pub async fn chooser_page(request: Request) -> reinhardt::http::Result<Response> {
	let Some(actor) = request
		.extensions
		.get::<crate::apps::identity::persistence::Actor>()
	else {
		return Ok(Response::new(http::StatusCode::SEE_OTHER).with_header("Location", "/login/"));
	};
	let state = match super::persistence::organizations_for(actor.id).await {
		Ok(items) => crate::client::screens::DashboardState::OrganizationRequired(items),
		Err(_) => crate::client::screens::DashboardState::Unavailable,
	};
	crate::client::document::respond(request, state).await
}
