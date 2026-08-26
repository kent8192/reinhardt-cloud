//! Dashboard shell layout with header, sidebar, and route outlet.

use reinhardt::pages::event::ClickEvent;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{Action, Outlet, Page, use_action};
use reinhardt::pages::server_fn::ServerFnError;

#[cfg(wasm)]
use crate::apps::auth::server_fn::logout::logout;
use crate::shared::client::routes::route_href;

fn nav_item_class(is_active: bool) -> &'static str {
	if is_active {
		"block rounded-md border border-control-500/20 bg-control-500/10 px-3 py-2 text-sm font-bold text-control-700 shadow-[inset_3px_0_0_#147d74]"
	} else {
		"block rounded-md border border-transparent px-3 py-2 text-sm font-semibold text-ink-600 hover:border-cloud-200 hover:bg-white hover:text-ink-950"
	}
}

fn route_is_active(current_path: &str, route_href: &str) -> bool {
	current_path
		.split_once('?')
		.map_or(current_path, |(path, _)| path)
		== route_href
}

#[cfg(wasm)]
async fn end_dashboard_session() -> Result<bool, ServerFnError> {
	logout().await
}

#[cfg(not(wasm))]
async fn end_dashboard_session() -> Result<bool, ServerFnError> {
	Ok(true)
}

#[cfg(wasm)]
fn replace_document(location: &str) -> Result<(), ServerFnError> {
	let window = web_sys::window()
		.ok_or_else(|| ServerFnError::server(500, "Browser window is unavailable"))?;
	window.location().replace(location).map_err(|error| {
		ServerFnError::server(500, format!("Unable to leave dashboard: {error:?}"))
	})
}

#[cfg(not(wasm))]
fn replace_document(_location: &str) -> Result<(), ServerFnError> {
	Ok(())
}

/// Render the shared dashboard chrome around its active child route.
#[reinhardt::pages::layout("/", name = "dashboard:layout")]
pub fn dashboard_layout(outlet: Outlet) -> Page {
	let login_href = route_href("auth:login_page", "/login");
	let logout_action: Action<bool, ServerFnError> = use_action({
		let login_href = login_href.clone();
		move |_: ()| {
			let login_href = login_href.clone();
			async move {
				let logged_out = end_dashboard_session().await?;
				replace_document(&login_href)?;
				Ok(logged_out)
			}
		}
	});
	let current_path = reinhardt::pages::app::try_with_spa_router(|router| *router.current_path());
	let account_href = route_href("auth:account_page", "/account");
	let home_href = route_href("dashboard:home", "/");
	let clusters_href = route_href("clusters:list", "/clusters");
	let deployments_href = route_href("deployments:list", "/deployments");
	let github_href = route_href("github:repositories", "/github");

	page!({
		{
			let current_path = current_path
				.map(|path| path.get())
				.unwrap_or_else(|| "/".to_string());
			let outlet = outlet.clone();
			let account_href = account_href.clone();
			let home_href = home_href.clone();
			let clusters_href = clusters_href.clone();
			let deployments_href = deployments_href.clone();
			let github_href = github_href.clone();
			let logout_action = logout_action;
			page!({
				div {
					class: "rc-app flex flex-col",
					header {
						class: "sticky top-0 z-10 h-16 border-b border-cloud-200 bg-white/90 backdrop-blur flex items-center justify-between px-4 sm:px-6 shrink-0",
						div {
							class: "flex items-center gap-3",
							span {
								class: "grid h-9 w-9 place-items-center rounded-md bg-ink-950 text-sm font-bold text-white shadow-[0_10px_20px_rgba(17,16,19,0.16)]",
								"RC"
							}
							div {
								span {
									class: "block text-base font-bold leading-tight text-ink-950",
									"Reinhardt Cloud"
								}
								span {
									class: "hidden text-xs font-semibold uppercase text-ink-600 sm:block",
									"Deploy control"
								}
							}
						}
						div {
							class: "flex items-center gap-2 sm:gap-3",
							span {
								class: "hidden rounded-md border border-cloud-200 bg-cloud-50 px-3 py-1.5 text-xs font-bold uppercase text-ink-600 sm:inline-flex",
								"Healthy"
							}
							a {
								href: account_href.clone(),
								class: "rc-link",
								"Account"
							}
							button {
								type: "button",
								class: "rc-link",
								@click: move |event: ClickEvent| {
									event.prevent_default();
									logout_action.dispatch(());
								},
								"Logout"
							}
						}
					}
					div {
						class: "flex flex-1 flex-col md:flex-row",
						nav {
							class: "box-border w-full border-b border-cloud-200 bg-cloud-50/85 p-4 shrink-0 md:min-h-[calc(100vh-4rem)] md:w-64 md:border-b-0 md:border-r md:bg-white/80",
							div {
								class: "mb-4 rounded-md border border-cloud-200 bg-white p-3",
								p {
									class: "text-xs font-bold uppercase text-ink-600",
									"Organization"
								}
								p {
									class: "mt-1 truncate text-sm font-bold text-ink-950",
									"current workspace"
								}
							}
							ul {
								class: "space-y-1.5",
								li {
									a {
										href: home_href,
										class: self::nav_item_class(self::route_is_active(&current_path, &home_href)),
										"Overview"
									}
								}
								li {
									a {
										href: clusters_href,
										class: self::nav_item_class(self::route_is_active(&current_path, &clusters_href)),
										"Clusters"
									}
								}
								li {
									a {
										href: deployments_href,
										class: self::nav_item_class(self::route_is_active(&current_path, &deployments_href)),
										"Deployments"
									}
								}
								li {
									a {
										href: github_href,
										class: self::nav_item_class(self::route_is_active(&current_path, &github_href)),
										"GitHub"
									}
								}
								li {
									a {
										href: account_href,
										class: self::nav_item_class(self::route_is_active(&current_path, &account_href)),
										"Account"
									}
								}
							}
						}
						main {
							class: "min-w-0 flex-1",
							{ outlet }
						}
					}
				}
			})
		}
	})
}

/// Render the main dashboard overview.
#[reinhardt::pages::component("/", name = "dashboard:home")]
pub fn dashboard_shell() -> Page {
	let clusters_href = route_href("clusters:list", "/clusters");
	let deployments_href = route_href("deployments:list", "/deployments");
	let github_href = route_href("github:repositories", "/github");
	page!({
		div {
			class: "rc-shell",
			div {
				class: "rc-topline",
				div {
					p {
						class: "rc-kicker",
						"Control plane"
					}
					h1 {
						class: "rc-title mt-1",
						"Deployment Operations"
					}
				}
				p {
					class: "rc-muted max-w-xl",
					"Live workspace for clusters, deployments, source imports, and account access."
				}
			}
			div {
				class: "grid grid-cols-1 gap-4 md:grid-cols-3",
				div {
					class: "rc-panel-pad border-l-4 border-l-control-500",
					h3 {
						class: "text-xs font-bold uppercase text-ink-600",
						"Clusters"
					}
					p {
						class: "mt-3 text-3xl font-bold text-ink-950",
						"0"
					}
					p {
						class: "mt-1 text-xs font-semibold text-ink-600",
						"registered targets"
					}
				}
				div {
					class: "rc-panel-pad border-l-4 border-l-relay-500",
					h3 {
						class: "text-xs font-bold uppercase text-ink-600",
						"Deployments"
					}
					p {
						class: "mt-3 text-3xl font-bold text-ink-950",
						"0"
					}
					p {
						class: "mt-1 text-xs font-semibold text-ink-600",
						"active releases"
					}
				}
				div {
					class: "rc-panel-pad border-l-4 border-l-signal-500",
					h3 {
						class: "text-xs font-bold uppercase text-ink-600",
						"System Status"
					}
					p {
						class: "mt-3 inline-flex rounded-full bg-control-500/10 px-2.5 py-1 text-sm font-bold text-control-700",
						"Healthy"
					}
					p {
						class: "mt-2 text-xs font-semibold text-ink-600",
						"router and websocket ready"
					}
				}
			}
			div {
				class: "mt-6 grid gap-4 lg:grid-cols-[1.2fr_0.8fr]",
				section {
					class: "rc-panel",
					div {
						class: "rc-panel-head",
						"Runbook"
					}
					div {
						class: "grid gap-0 divide-y divide-cloud-200",
						a {
							href: clusters_href.clone(),
							class: "flex items-center justify-between px-4 py-3 text-sm font-semibold text-ink-800 hover:bg-cloud-50",
							"Register cluster" span {
								class: "text-control-700",
								"Open"
							}
						}
						a {
							href: deployments_href.clone(),
							class: "flex items-center justify-between px-4 py-3 text-sm font-semibold text-ink-800 hover:bg-cloud-50",
							"Create deployment" span {
								class: "text-control-700",
								"Open"
							}
						}
						a {
							href: github_href.clone(),
							class: "flex items-center justify-between px-4 py-3 text-sm font-semibold text-ink-800 hover:bg-cloud-50",
							"Import repository" span {
								class: "text-control-700",
								"Open"
							}
						}
					}
				}
				section {
					class: "rc-panel-pad bg-ink-950 text-white",
					p {
						class: "text-xs font-bold uppercase text-cloud-200",
						"Control Surface"
					}
					p {
						class: "mt-3 text-2xl font-bold",
						"Dogfood-ready"
					}
					p {
						class: "mt-2 text-sm text-cloud-100",
						"Dashboard routes are rendered through the shared Reinhardt application shell."
					}
				}
			}
		}
	})
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	#[rstest]
	#[case::overview("/", "/", true)]
	#[case::account("/account", "/account", true)]
	#[case::clusters("/clusters", "/clusters", true)]
	#[case::deployment_logs("/deployments?logs=42", "/deployments", true)]
	#[case::different_route("/github", "/clusters", false)]
	fn route_active_state_matches_path(
		#[case] path: &str,
		#[case] route: &str,
		#[case] expected: bool,
	) {
		// Arrange
		let active = super::route_is_active(path, route);

		// Assert
		assert_eq!(active, expected);
	}

	#[rstest]
	fn dashboard_shell_renders_overview_links() {
		// Arrange
		let shell = super::dashboard_shell(super::DashboardShellProps {});

		// Act
		let html = shell.render_to_string();

		// Assert
		assert!(html.contains(r#"href="/clusters""#));
		assert!(html.contains(r#"href="/deployments""#));
		assert!(html.contains(r#"href="/github""#));
	}
}
