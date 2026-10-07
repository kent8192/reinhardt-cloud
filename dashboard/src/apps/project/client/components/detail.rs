//! Project detail route delegates Environment and operation rendering to their Apps.
use super::environment::environment_card;
use crate::apps::project::functions::load_project;
use crate::apps::project::services::ProjectDetail;
use crate::client::components::shell::STYLES;
use crate::client::navigation::{projects_path, route_link};
use crate::client::screens::{context, csrf_token, loading, query_error, workspace};
use reinhardt::pages::i18n::I18nContext;
use reinhardt::pages::{Page, Path, QueryOptions, component, page, use_query};
use uuid::Uuid;
#[component("projects/{project_id}/", name = "project-detail")]
pub fn project(Path(organization_id): Path<Uuid>, Path(project_id): Path<Uuid>) -> Page {
	let context = context();
	let query = use_query(
		load_project::query(organization_id, project_id),
		QueryOptions::default(),
	);
	page!({
		if let Some(detail) = query.data() {
			{
				workspace(
					Some(detail.name.clone()),
					project_detail_view(detail.clone(), context.clone()),
					true,
					context.clone(),
					csrf_token(),
				)
			}
		} else if let Some(error) = query.error() {
			{
				workspace(
					None,
					query_error(error.clone(), context.clone()),
					true,
					context.clone(),
					csrf_token(),
				)
			}
		} else {
			{
				workspace(
					None,
					loading(context.clone()),
					true,
					context.clone(),
					csrf_token(),
				)
			}
		}
	})
}
pub fn project_detail_view(detail: ProjectDetail, context: I18nContext) -> Page {
	let ProjectDetail {
		id: project_id,
		organization_id,
		repository,
		environments: rows,
		..
	} = detail;
	let empty = rows.is_empty();
	let environments = rows
		.into_iter()
		.map(|environment| {
			(
				environment.id,
				environment_card(organization_id, project_id, environment, context.clone()),
			)
		})
		.collect::<Vec<_>>();
	page!({
		nav {
			aria_label: "Project navigation",
			{ route_link(
				projects_path(organization_id),
				context.translate("All projects"),
			) }
		}
		p {
			class: STYLES.copy(),
			{ repository }
		}
		h2 {
			class: STYLES.panel_title(),
			{ context.translate("Environments") }
		}
		p {
			class: STYLES.copy(),
			{ context.translate("Desired inputs. Readiness is reported separately.") }
		}
		if empty {
			p {
				class: STYLES.copy(),
				{ context.translate("No environments yet") }
			}
		}
		div {
			class: STYLES.environment_grid(),
			for environment in environments @key(environment.0) { { environment.1 } }
		}
	})
}
