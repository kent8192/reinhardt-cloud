//! Dashboard shell layout with header, sidebar, and route outlet.

use std::time::Duration;

use reinhardt::pages::component;
use reinhardt::pages::event::ClickEvent;
use reinhardt::pages::prelude::{
	ClassToken, NavigationContext, NavigationDecision, NavigationGuardError, Outlet, Page,
	QueryOptions, QuerySnapshot, QueryStatus, use_query, use_server_mutation,
};
use reinhardt::pages::server_fn::ServerFnError;
use reinhardt::pages::{NavigationType, navigate_or_reload, navigation_guard, page};

use crate::apps::auth::server_fn::logout::logout;
use crate::apps::auth::server_fn::me::me;
use crate::apps::clusters::server_fn::list_clusters_for_current_org;
use crate::apps::dashboard::client::style::STYLES;
use crate::apps::deployments::server_fn::list_deployments_for_current_org;
use crate::shared::UserInfo;
use crate::shared::client::routes::route_href;
use crate::shared::client::style::STYLES as SHARED_STYLES;

#[derive(Debug, PartialEq)]
enum DashboardGate {
	Waiting,
	Authenticated,
	LoginRequired,
	Failed(String),
}

const SESSION_REVALIDATION_INTERVAL: Duration = Duration::from_secs(60);

fn dashboard_gate(snapshot: QuerySnapshot<UserInfo, ServerFnError>) -> DashboardGate {
	if let Some(error) = snapshot.error.or(snapshot.refetch_error) {
		return match error.status() {
			Some(401 | 403) => {
				crate::shared::client::ws::disconnect_notifications();
				DashboardGate::LoginRequired
			}
			_ => DashboardGate::Failed(error.user_message().to_string()),
		};
	}
	if snapshot.is_fetching || snapshot.is_stale {
		return DashboardGate::Waiting;
	}

	match snapshot.status {
		QueryStatus::Idle | QueryStatus::Pending => DashboardGate::Waiting,
		QueryStatus::Success if snapshot.data.is_some() => DashboardGate::Authenticated,
		QueryStatus::Success | QueryStatus::Error => {
			DashboardGate::Failed("Dashboard session could not be verified.".to_string())
		}
	}
}

fn nav_item_class(is_active: bool) -> ClassToken {
	if is_active {
		STYLES.navigation_item_active()
	} else {
		STYLES.navigation_item()
	}
}

fn route_is_active(current_path: &str, route_href: &str) -> bool {
	current_path
		.split_once('?')
		.map_or(current_path, |(path, _)| path)
		== route_href
}

fn dashboard_navigation_decision(
	result: Result<(), ServerFnError>,
) -> Result<NavigationDecision, NavigationGuardError> {
	if result
		.as_ref()
		.is_err_and(|error| matches!(error.status(), Some(401 | 403)))
	{
		crate::shared::client::ws::disconnect_notifications();
	}
	match result {
		Ok(()) => Ok(NavigationDecision::Allow),
		Err(error) => match error.status() {
			Some(401) => Ok(NavigationDecision::Redirect {
				location: route_href("auth:login_page", "/login"),
				replace: true,
			}),
			Some(403) => Ok(NavigationDecision::Forbidden),
			_ => Err(NavigationGuardError::from_diagnostic(
				error.user_message().to_owned(),
				error.status(),
				error,
			)),
		},
	}
}

/// Verify the browser session before a protected layout or child is mounted.
///
/// The static dashboard shell authenticates in the browser. Native route
/// execution has no browser cookie context and therefore fails closed.
#[navigation_guard]
pub(crate) async fn require_dashboard_session(
	_context: NavigationContext,
) -> Result<NavigationDecision, NavigationGuardError> {
	#[cfg(wasm)]
	{
		dashboard_navigation_decision(me().await.map(|_| ()))
	}
	#[cfg(not(wasm))]
	{
		dashboard_navigation_decision(Err(ServerFnError::auth(
			401,
			"A browser session is required",
		)))
	}
}

/// Render the shared dashboard chrome around its active child route.
#[reinhardt::pages::layout(
	"/",
	name = "dashboard:layout",
	navigation_guard = require_dashboard_session,
)]
pub fn dashboard_layout(outlet: Outlet) -> Page {
	let login_href = route_href("auth:login_page", "/login");
	let logout_action = use_server_mutation({
		let login_href = login_href.clone();
		let logout = logout::mutation();
		move |()| {
			let login_href = login_href.clone();
			let request = logout(());
			async move {
				let logged_out = request.await?;
				crate::shared::client::ws::disconnect_notifications();
				reinhardt::pages::auth::auth_state().logout();
				// This evicts all query families and cancels in-flight reads before navigation.
				reinhardt::pages::auth::invalidate_authentication();
				navigate_or_reload(login_href, NavigationType::Replace).map_err(|error| {
					ServerFnError::application(format!("Unable to leave dashboard: {error}"))
				})?;
				Ok::<_, ServerFnError>(logged_out)
			}
		}
	})
	.build();
	let current_path = reinhardt::pages::app::try_with_spa_router(|router| *router.current_path());
	let account_href = route_href("auth:account_page", "/account");
	let home_href = route_href("dashboard:home", "/");
	let clusters_href = route_href("clusters:list", "/clusters");
	let deployments_href = route_href("deployments:list", "/deployments");
	let github_href = route_href("github:repositories", "/github");
	let session = use_query(
		me::query(),
		QueryOptions::new()
			.enabled(cfg!(wasm))
			.refetch_interval(SESSION_REVALIDATION_INTERVAL),
	);

	page!({
		div { {
			let gate = match self::dashboard_gate(session.snapshot()) {
				DashboardGate::LoginRequired if navigate_or_reload(login_href.clone(), NavigationType::Replace).is_err() => {
					DashboardGate::Failed("Unable to redirect to sign in.".to_string())
				}
				gate => gate,
			};
			match gate {
				DashboardGate::Authenticated => Page::Empty,
				gate => {
					let (title, message, role) = match gate {
						DashboardGate::Waiting => (
							"Verifying dashboard session",
							"Please wait while your session is checked.".to_string(),
							"status",
						),
						DashboardGate::LoginRequired => (
							"Redirecting to sign in",
							"Your dashboard session is no longer available.".to_string(),
							"status",
						),
						DashboardGate::Failed(message) => {
							("Dashboard unavailable", message, "alert")
						}
						DashboardGate::Authenticated => unreachable!(),
					};
					page!({
					main {
						class: SHARED_STYLES.app(),
						role: role,
						div {
							class: SHARED_STYLES.shell(),
							section {
								class: SHARED_STYLES.panel_pad(),
								h1 {
									class: SHARED_STYLES.title(),
									{ title }
								}
								p {
									class: SHARED_STYLES.muted(),
									{ message }
								}
							}
						}
					}
					})
				}
			}
		}{
				// Update visibility without rebuilding the active route's form controls.
				let shell_session = session.clone();
				Page::element("div")
					.attr("class", (SHARED_STYLES.app() + STYLES.dashboard_app()).as_str().to_owned())
					.reactive_attr("hidden", move || {
						(!matches!(
							self::dashboard_gate(shell_session.snapshot()),
							DashboardGate::Authenticated
						)).then_some("hidden".into())
					})
					.child(page!({
				header {
					class: STYLES.dashboard_header(),
					div {
						class: STYLES.header_brand(),
						span {
							class: STYLES.brand_mark(),
							"RC"
						}
						div {
							span {
								class: STYLES.brand_name(),
								"Reinhardt Cloud"
							}
							span {
								class: STYLES.brand_subtitle(),
								"Deploy control"
							}
						}
					}
					div {
						class: STYLES.header_actions(),
						a {
							href: account_href.clone(),
							class: SHARED_STYLES.link() + STYLES.header_action(),
							"Account"
						}
						button {
							type: "button",
							class: SHARED_STYLES.link() + STYLES.header_action(),
							disabled: logout_action.is_pending(),
							@click: move |event: ClickEvent| {
								event.prevent_default();
								logout_action.dispatch(());
							},
							"Logout"
						}
					}
				}
				div {
					class: STYLES.dashboard_body(),
					nav {
						class: STYLES.sidebar(),
						div {
							class: STYLES.organization(),
							p {
								class: STYLES.organization_label(),
								"Organization"
							}
							p {
								class: STYLES.organization_name(),
								"current workspace"
							}
						}
						ul {
							class: STYLES.navigation_list(),
							li {
								a {
									href: home_href.clone(),
									class: {
										let path = current_path
											.map(|path| path.get())
											.unwrap_or_else(|| "/".to_string());
										self::nav_item_class(self::route_is_active(&path, &home_href))
									},
									"Overview"
								}
							}
							li {
								a {
									href: clusters_href.clone(),
									class: {
										let path = current_path
											.map(|path| path.get())
											.unwrap_or_else(|| "/".to_string());
										self::nav_item_class(self::route_is_active(&path, &clusters_href))
									},
									"Clusters"
								}
							}
							li {
								a {
									href: deployments_href.clone(),
									class: {
										let path = current_path
											.map(|path| path.get())
											.unwrap_or_else(|| "/".to_string());
										self::nav_item_class(self::route_is_active(&path, &deployments_href))
									},
									"Deployments"
								}
							}
							li {
								a {
									href: github_href.clone(),
									class: {
										let path = current_path
											.map(|path| path.get())
											.unwrap_or_else(|| "/".to_string());
										self::nav_item_class(self::route_is_active(&path, &github_href))
									},
									"GitHub"
								}
							}
							li {
								a {
									href: account_href.clone(),
									class: {
										let path = current_path
											.map(|path| path.get())
											.unwrap_or_else(|| "/".to_string());
										self::nav_item_class(self::route_is_active(&path, &account_href))
									},
									"Account"
								}
							}
						}
					}
					main {
						class: STYLES.dashboard_main(),
						{ outlet }
					}
				}
					}))
			} }
	})
}

fn overview_count<T: Clone>(snapshot: QuerySnapshot<Vec<T>, ServerFnError>) -> String {
	if snapshot.error.is_some() || snapshot.refetch_error.is_some() {
		return "Unavailable".to_owned();
	}
	match snapshot.data {
		Some(items) => items.len().to_string(),
		None if snapshot.is_fetching => "Loading...".to_owned(),
		None => "Unavailable".to_owned(),
	}
}

/// Render the main dashboard overview.
#[component("/", name = "dashboard:home")]
pub fn dashboard_shell() -> Page {
	let clusters = use_query(
		list_clusters_for_current_org::query(),
		QueryOptions::new().enabled(cfg!(wasm)),
	);
	let deployments = use_query(
		list_deployments_for_current_org::query(),
		QueryOptions::new().enabled(cfg!(wasm)),
	);
	let clusters_href = route_href("clusters:list", "/clusters");
	let deployments_href = route_href("deployments:list", "/deployments");
	let github_href = route_href("github:repositories", "/github");
	page!({
		div {
			class: SHARED_STYLES.shell(),
			div {
				class: SHARED_STYLES.topline(),
				div {
					p {
						class: SHARED_STYLES.kicker(),
						"Control plane"
					}
					h1 {
						class: SHARED_STYLES.title() + STYLES.overview_title(),
						"Deployment Operations"
					}
				}
				p {
					class: SHARED_STYLES.muted() + STYLES.overview_description(),
					"Live workspace for clusters, deployments, source imports, and account access."
				}
			}
			div {
				class: STYLES.overview_metrics(),
				div {
					class: SHARED_STYLES.panel_pad() + STYLES.metric_card() + STYLES.metric_clusters(),
					h3 {
						class: STYLES.metric_label(),
						"Clusters"
					}
					p {
						class: STYLES.metric_value(),
						{ self::overview_count(clusters.snapshot()) }
					}
					p {
						class: STYLES.metric_detail(),
						"registered targets"
					}
				}
				div {
					class: SHARED_STYLES.panel_pad() + STYLES.metric_card() + STYLES.metric_deployments(),
					h3 {
						class: STYLES.metric_label(),
						"Deployments"
					}
					p {
						class: STYLES.metric_value(),
						{ self::overview_count(deployments.snapshot()) }
					}
					p {
						class: STYLES.metric_detail(),
						"registered deployments"
					}
				}
			}
			div {
				class: STYLES.overview_panels(),
				section {
					class: SHARED_STYLES.panel(),
					div {
						class: SHARED_STYLES.panel_head(),
						"Runbook"
					}
					div {
						class: STYLES.runbook_list(),
						a {
							href: clusters_href.clone(),
							class: STYLES.runbook_link(),
							"Register cluster" span {
								class: STYLES.runbook_link_action(),
								"Open"
							}
						}
						a {
							href: deployments_href.clone(),
							class: STYLES.runbook_link(),
							"Create deployment" span {
								class: STYLES.runbook_link_action(),
								"Open"
							}
						}
						a {
							href: github_href.clone(),
							class: STYLES.runbook_link(),
							"Import repository" span {
								class: STYLES.runbook_link_action(),
								"Open"
							}
						}
					}
				}
				section {
					class: SHARED_STYLES.panel_pad() + STYLES.control_surface(),
					p {
						class: STYLES.control_surface_label(),
						"Control Surface"
					}
					p {
						class: STYLES.control_surface_title(),
						"Dogfood-ready"
					}
					p {
						class: STYLES.control_surface_description(),
						"Dashboard routes are rendered through the shared Reinhardt application shell."
					}
				}
			}
		}
	})
}

#[cfg(test)]
mod tests {
	use reinhardt::pages::prelude::{NavigationDecision, QuerySnapshot, QueryStatus};
	use reinhardt::pages::server_fn::ServerFnError;
	use rstest::rstest;

	use crate::shared::UserInfo;

	use super::DashboardGate;

	fn session_snapshot(
		status: QueryStatus,
		has_user: bool,
		error: Option<ServerFnError>,
		refetch_error: Option<ServerFnError>,
	) -> QuerySnapshot<UserInfo, ServerFnError> {
		QuerySnapshot {
			status,
			data: has_user.then(|| UserInfo {
				id: "user-1".to_string(),
				username: "alice".to_string(),
				email: "alice@example.com".to_string(),
			}),
			error,
			refetch_error,
			is_fetching: status == QueryStatus::Pending,
			is_stale: false,
		}
	}

	#[rstest]
	#[case::empty(Some(vec![]), false, false, "0")]
	#[case::populated(Some(vec![1, 2, 3]), false, false, "3")]
	#[case::loading(None, true, false, "Loading...")]
	#[case::not_loaded(None, false, false, "Unavailable")]
	#[case::failed(None, false, true, "Unavailable")]
	#[case::failed_refresh(Some(vec![1, 2]), false, true, "Unavailable")]
	fn overview_counts_reflect_query_results(
		#[case] data: Option<Vec<i32>>,
		#[case] is_fetching: bool,
		#[case] failed: bool,
		#[case] expected: &str,
	) {
		// Arrange
		let snapshot = QuerySnapshot {
			status: if data.is_some() {
				QueryStatus::Success
			} else {
				QueryStatus::Pending
			},
			data,
			error: None,
			refetch_error: failed.then(|| ServerFnError::server(503, "Unavailable")),
			is_fetching,
			is_stale: false,
		};

		// Act
		let count = super::overview_count(snapshot);

		// Assert
		assert_eq!(count, expected);
	}

	#[rstest]
	#[case::authenticated(Ok(()), NavigationDecision::Allow)]
	#[case::anonymous(
		Err(ServerFnError::auth(401, "Session expired")),
		NavigationDecision::Redirect { location: "/login".to_owned(), replace: true }
	)]
	#[case::forbidden(
		Err(ServerFnError::auth(403, "Account disabled")),
		NavigationDecision::Forbidden
	)]
	fn navigation_checks_session_before_mounting(
		#[case] result: Result<(), ServerFnError>,
		#[case] expected: NavigationDecision,
	) {
		// Arrange + Act
		let decision = super::dashboard_navigation_decision(result);

		// Assert
		assert_eq!(decision, Ok(expected));
	}

	#[rstest]
	fn navigation_preserves_safe_service_errors() {
		// Arrange
		let result = Err(ServerFnError::server(
			503,
			"Dashboard is temporarily unavailable.",
		));

		// Act
		let error = super::dashboard_navigation_decision(result).unwrap_err();

		// Assert
		assert_eq!(error.status(), Some(503));
		assert_eq!(
			error.public_message(),
			"Dashboard is temporarily unavailable."
		);
	}

	#[rstest]
	#[case::disabled(QueryStatus::Idle, false, None, None, DashboardGate::Waiting)]
	#[case::initial_fetch(QueryStatus::Pending, false, None, None, DashboardGate::Waiting)]
	#[case::authenticated(QueryStatus::Success, true, None, None, DashboardGate::Authenticated)]
	#[case::missing_success_data(
		QueryStatus::Success,
		false,
		None,
		None,
		DashboardGate::Failed("Dashboard session could not be verified.".to_string())
	)]
	#[case::unauthorized(
		QueryStatus::Error,
		false,
		Some(ServerFnError::auth(401, "Session expired")),
		None,
		DashboardGate::LoginRequired
	)]
	#[case::forbidden(
		QueryStatus::Error,
		false,
		Some(ServerFnError::auth(403, "Account disabled")),
		None,
		DashboardGate::LoginRequired
	)]
	#[case::expired_refetch(
		QueryStatus::Success,
		true,
		None,
		Some(ServerFnError::auth(401, "Session expired")),
		DashboardGate::LoginRequired
	)]
	#[case::service_failure(
		QueryStatus::Error,
		false,
		Some(ServerFnError::server(503, "Dashboard is temporarily unavailable.")),
		None,
		DashboardGate::Failed("Dashboard is temporarily unavailable.".to_string())
	)]
	#[case::refetch_failure(
		QueryStatus::Success,
		true,
		None,
		Some(ServerFnError::server(503, "Dashboard is temporarily unavailable.")),
		DashboardGate::Failed("Dashboard is temporarily unavailable.".to_string())
	)]
	#[case::missing_error(
		QueryStatus::Error,
		false,
		None,
		None,
		DashboardGate::Failed("Dashboard session could not be verified.".to_string())
	)]
	fn session_gate_selects_protected_layout_state(
		#[case] status: QueryStatus,
		#[case] has_user: bool,
		#[case] error: Option<ServerFnError>,
		#[case] refetch_error: Option<ServerFnError>,
		#[case] expected: DashboardGate,
	) {
		// Arrange
		let snapshot = session_snapshot(status, has_user, error, refetch_error);

		// Act
		let actual = super::dashboard_gate(snapshot);

		// Assert
		assert_eq!(actual, expected);
	}

	#[rstest]
	#[case::background_refetch(true, false)]
	#[case::stale_cached_user(false, true)]
	fn session_gate_hides_cached_user_until_revalidation_finishes(
		#[case] is_fetching: bool,
		#[case] is_stale: bool,
	) {
		// Arrange
		let mut snapshot = session_snapshot(QueryStatus::Success, true, None, None);
		snapshot.is_fetching = is_fetching;
		snapshot.is_stale = is_stale;

		// Act
		let actual = super::dashboard_gate(snapshot);

		// Assert
		assert_eq!(actual, DashboardGate::Waiting);
	}

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

	#[cfg(native)]
	#[rstest]
	fn dashboard_shell_renders_overview_links() {
		// Arrange
		let mut html = String::new();

		// Act
		let _screen = reinhardt::pages::testing::component::render(|| {
			let shell = super::dashboard_shell(super::DashboardShellProps {});
			html = shell.render_to_string();
			shell
		});

		// Assert
		let hrefs = html
			.split("href=\"")
			.skip(1)
			.map(|fragment| fragment.split('"').next().unwrap_or_default())
			.collect::<Vec<_>>();
		assert_eq!(hrefs, vec!["/clusters", "/deployments", "/github"]);
		let metrics_classes = (super::SHARED_STYLES.panel_pad()
			+ super::STYLES.metric_card()
			+ super::STYLES.metric_clusters())
		.as_str()
		.to_owned();
		let expected_metrics = format!(
			"<div class=\"{}\"><div class=\"{metrics_classes}\">",
			super::STYLES.overview_metrics().as_str()
		);
		assert!(
			html.contains(&expected_metrics),
			"metrics grid owns the cluster metric card"
		);
		let expected_runbook_link = format!(
			"<a href=\"/clusters\" class=\"{}\">Register cluster",
			super::STYLES.runbook_link().as_str()
		);
		assert!(
			html.contains(&expected_runbook_link),
			"runbook link owns its generated token"
		);
	}

	#[rstest]
	fn nav_item_class_selects_the_exact_generated_token() {
		// Arrange + Act
		let active_class = super::nav_item_class(true);
		let inactive_class = super::nav_item_class(false);

		// Assert
		assert_eq!(
			active_class.as_str(),
			super::STYLES.navigation_item_active().as_str()
		);
		assert_eq!(
			inactive_class.as_str(),
			super::STYLES.navigation_item().as_str()
		);
	}
}
