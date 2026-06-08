//! Dashboard shell layout with header, sidebar, and main content area.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;

fn nav_item_class(is_active: bool) -> &'static str {
	if is_active {
		"block rounded-md bg-control-500/10 px-3 py-2 text-sm font-semibold text-control-700"
	} else {
		"block rounded-md px-3 py-2 text-sm font-medium text-ink-600 hover:bg-cloud-100 hover:text-ink-950"
	}
}

/// Render the shared dashboard application chrome around a section page.
pub fn dashboard_app_shell(active_item: &'static str, content: Page) -> Page {
	let account_href = "/account".to_string();
	let home_href = "/".to_string();
	let clusters_href = "/clusters".to_string();
	let deployments_href = "/deployments".to_string();
	let github_href = "/github".to_string();
	page!(|active_item: &'static str, content: Page, account_href: String, home_href: String, clusters_href: String, deployments_href: String, github_href: String| {
		div {
			class: "rc-app flex flex-col",
			header {
				class: "h-14 border-b border-cloud-200 bg-white/95 flex items-center justify-between px-6 shrink-0",
				div {
					class: "flex items-center gap-3",
					span {
						class: "h-2.5 w-2.5 rounded-full bg-control-500 shadow-[0_0_0_4px_rgba(15,118,110,0.12)]",
					}
					span {
						class: "text-lg font-semibold text-ink-950",
						"Reinhardt Cloud"
					}
				}
				div {
					class: "flex items-center gap-4",
					span {
						class: "text-sm text-ink-600",
						"Dashboard"
					}
					a {
						href: account_href.clone(),
						class: "rc-link",
						"Account"
					}
					button {
						type: "button",
						class: "rc-link js-dashboard-logout",
						"Logout"
					}
				}
			}
			div {
				class: "flex flex-1 flex-col md:flex-row",
				nav {
					class: "box-border w-full border-b border-cloud-200 bg-white p-4 shrink-0 md:w-56 md:border-b-0 md:border-r",
					ul {
						class: "space-y-1",
						li {
							a {
								href: home_href,
								class: self::nav_item_class(active_item == "overview"),
								"Overview"
							}
						}
						li {
							a {
								href: clusters_href,
								class: self::nav_item_class(active_item == "clusters"),
								"Clusters"
							}
						}
						li {
							a {
								href: deployments_href,
								class: self::nav_item_class(active_item == "deployments"),
								"Deployments"
							}
						}
						li {
							a {
								href: github_href,
								class: self::nav_item_class(active_item == "github"),
								"GitHub"
							}
						}
						li {
							a {
								href: account_href,
								class: self::nav_item_class(active_item == "account"),
								"Account"
							}
						}
					}
				}
				main {
					class: "flex-1 p-6",
					{ content }
				}
			}
		}
	})(
		active_item,
		content,
		account_href,
		home_href,
		clusters_href,
		deployments_href,
		github_href,
	)
}

/// Render the main dashboard shell with navigation sidebar and overview cards.
pub fn dashboard_shell() -> Page {
	let content = page!(|| {
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
						"Dashboard"
					}
				}
				p {
					class: "rc-muted max-w-xl",
					"Operational entry point for clusters, deployments, and platform health."
				}
			}
			div {
				class: "grid grid-cols-1 gap-4 md:grid-cols-3",
				div {
					class: "rc-panel-pad",
					h3 {
						class: "text-xs font-semibold uppercase tracking-[0.12em] text-ink-600",
						"Clusters"
					}
					p {
						class: "mt-3 text-3xl font-semibold text-ink-950",
						"0"
					}
				}
				div {
					class: "rc-panel-pad",
					h3 {
						class: "text-xs font-semibold uppercase tracking-[0.12em] text-ink-600",
						"Deployments"
					}
					p {
						class: "mt-3 text-3xl font-semibold text-ink-950",
						"0"
					}
				}
				div {
					class: "rc-panel-pad",
					h3 {
						class: "text-xs font-semibold uppercase tracking-[0.12em] text-ink-600",
						"System Status"
					}
					p {
						class: "mt-3 inline-flex rounded-full bg-control-500/10 px-2.5 py-1 text-sm font-semibold text-control-700",
						"Healthy"
					}
				}
			}
		}
	})();
	dashboard_app_shell("overview", content)
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	#[rstest]
	#[case::active(
		true,
		"block rounded-md bg-control-500/10 px-3 py-2 text-sm font-semibold text-control-700"
	)]
	#[case::inactive(
		false,
		"block rounded-md px-3 py-2 text-sm font-medium text-ink-600 hover:bg-cloud-100 hover:text-ink-950"
	)]
	fn nav_item_class_reflects_active_state(#[case] is_active: bool, #[case] expected: &str) {
		// Arrange
		let expected_class = expected;

		// Act
		let class = super::nav_item_class(is_active);

		// Assert
		assert_eq!(class, expected_class);
	}

	#[rstest]
	fn dashboard_shell_renders_account_and_logout_controls() {
		use reinhardt::pages::component::Page;

		// Arrange
		let content = Page::Empty;

		// Act
		let html = super::dashboard_app_shell("account", content).render_to_string();

		// Assert
		assert!(html.contains(r#"href="/account""#));
		assert!(html.contains("Account"));
		assert!(html.contains("js-dashboard-logout"));
		assert!(html.contains("Logout"));
		assert!(!html.contains(">Login<"));
	}
}
