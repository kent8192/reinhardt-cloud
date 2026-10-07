//! Organization-scoped Project list route and presentation.
use crate::apps::project::functions::load_projects;
use crate::apps::project::services::ProjectSummary;
use crate::client::components::shell::STYLES;
use crate::client::navigation::{detail_path, route_link};
use crate::client::screens::{context, csrf_token, loading, query_error, workspace};
use reinhardt::pages::i18n::I18nContext;
use reinhardt::pages::{Page, Path, QueryOptions, component, page, use_query};
use uuid::Uuid;
#[component("projects/", name = "project-list")]
pub fn projects(Path(organization_id): Path<Uuid>) -> Page {
	let context = context();
	let query = use_query(
		load_projects::query(organization_id),
		QueryOptions::default(),
	);
	let content = page!({
		if let Some(items) = query.data() {
			{ project_table(items.clone(), context.clone()) }
		} else if let Some(error) = query.error() {
			{ query_error(error.clone(), context.clone()) }
		} else {
			{ loading(context.clone()) }
		}
	});
	workspace(None, content, true, context, csrf_token())
}
pub fn project_table(items: Vec<ProjectSummary>, context: I18nContext) -> Page {
	if items.is_empty() {
		return page!({
			h2 {
				class: STYLES.panel_title(),
				{ context.translate("No projects yet") }
			}
			p {
				class: STYLES.copy(),
				{ context.translate("Connect a repository to publish your first application.") }
			}
		});
	}
	page!({
		table {
			class: STYLES.table(),
			thead {
				tr {
					th {
						scope: "col",
						{ context.translate("Project") }
					}
					th {
						scope: "col",
						{ context.translate("Repository") }
					}
					th {
						scope: "col",
						{ context.translate("Environments") }
					}
				}
			}
			tbody {
				for item in items @key(item.id) {
					tr {
						td { { route_link(
							detail_path(item.organization_id, item.id),
							item.name.clone(),
						) } }
						td { { item.repository } }
						td { { format!("{}", item.environments) } }
					}
				}
			}
		}
	})
}
