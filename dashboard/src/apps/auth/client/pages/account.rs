//! Account page for profile and OAuth account linking.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{QueryHandle, QueryOptions, QuerySnapshot, QueryStatus, use_query};
use reinhardt::pages::server_fn::ServerFnError;

use crate::apps::auth::server_fn::linked_accounts::{
	LinkedOAuthAccountInfo, list_linked_oauth_accounts,
};
use crate::apps::auth::server_fn::me::me;
use crate::apps::auth::server_fn::oauth_providers::{OAuthProviderInfo, list_oauth_providers};
use crate::shared::UserInfo;
use crate::shared::client::routes::route_href;

fn github_link_url(providers: &[OAuthProviderInfo]) -> Option<String> {
	providers
		.iter()
		.find(|provider| provider.id == "github")
		.map(|provider| {
			let separator = if provider.start_url.contains('?') {
				"&"
			} else {
				"?"
			};
			format!("{}{}intent=link", provider.start_url, separator)
		})
}

fn github_account(linked: &[LinkedOAuthAccountInfo]) -> Option<&LinkedOAuthAccountInfo> {
	linked.iter().find(|account| account.provider == "github")
}

pub(crate) fn render_account_content(
	user: UserInfo,
	providers: Vec<OAuthProviderInfo>,
	linked: Vec<LinkedOAuthAccountInfo>,
) -> Page {
	let github = github_account(&linked).cloned();
	let github_linked = github.is_some();
	let github_label = github
		.and_then(|account| account.provider_username)
		.unwrap_or_else(|| "Linked".to_string());
	let github_link_url = github_link_url(&providers);
	page!({
		div {
			class: "rc-shell",
			div {
				class: "rc-topline",
				div {
					p {
						class: "rc-kicker",
						"Account"
					}
					h1 {
						class: "rc-title mt-1",
						"Account"
					}
				}
			}
			div {
				class: "grid gap-4 lg:grid-cols-2",
				section {
					class: "rc-panel-pad",
					h2 {
						class: "text-base font-semibold text-ink-950",
						"Profile"
					}
					dl {
						class: "mt-4 grid gap-3 text-sm",
						div {
							dt {
								class: "font-medium text-ink-600",
								"Username"
							}
							dd {
								class: "mt-1 text-ink-950",
								{ user.username.clone() }
							}
						}
						div {
							dt {
								class: "font-medium text-ink-600",
								"Email"
							}
							dd {
								class: "mt-1 text-ink-950",
								{ user.email.clone() }
							}
						}
					}
				}
				section {
					class: "rc-panel-pad",
					div {
						class: "flex items-start justify-between gap-4",
						div {
							h2 {
								class: "text-base font-semibold text-ink-950",
								"GitHub"
							}
							p {
								class: "rc-muted mt-1",
								"Authentication provider"
							}
						} {
							if github_linked {
								page!( {
									span {
										class: "inline-flex shrink-0 rounded-full bg-control-500/10 px-2.5 py-1 text-xs font-semibold text-control-700",
										{ github_label }
									}
								})
							} else { Page::Empty }
						}
					}
					div {
						class: "mt-5",
						{
							if github_linked {
								page!( {
									p {
										class: "text-sm font-medium text-ink-700",
										"GitHub account linked"
									}
								})
							} else if let Some(url) = github_link_url.clone() {
								page!( {
									a {
										href: url,
										rel: "external",
										class: "btn-primary inline-flex px-4 py-2 text-sm",
										"Link GitHub"
									}
								})
							} else {
								page!( {
									p {
										class: "text-sm font-medium text-ink-600",
										"GitHub OAuth is not configured"
									}
								})
							}
						}
					}
				}
			}
		}
	})
}

fn account_error(message: &str) -> Page {
	let login_href = route_href("auth:login_page", "/login");
	let message = message.to_string();
	page!({
		div {
			class: "rc-shell",
			div {
				class: "rc-panel-pad",
				h1 {
					class: "text-xl font-semibold text-ink-950",
					"Account"
				}
				p {
					class: "rc-muted mt-2",
					{ message }
				}
				a {
					href: login_href,
					class: "btn-primary mt-5 inline-flex px-4 py-2 text-sm",
					"Sign in"
				}
			}
		}
	})
}

fn account_loading(message: &'static str) -> Page {
	page!({
		div {
			class: "rc-shell",
			div {
				class: "rc-panel-pad",
				p {
					class: "rc-muted text-sm",
					{ message }
				}
			}
		}
	})
}

fn query_error_message(error: Option<ServerFnError>, fallback: &'static str) -> String {
	error
		.map(|error| error.user_message().to_string())
		.unwrap_or_else(|| fallback.to_string())
}

fn query_refresh_notice(
	is_fetching: bool,
	refetch_error: Option<ServerFnError>,
	label: &'static str,
) -> Page {
	if let Some(error) = refetch_error {
		let message = format!(
			"Showing cached {label}; the latest refresh failed: {}",
			error.user_message()
		);
		return page!({
			div {
				class: "border-b border-amber-100 bg-amber-50 px-4 py-2 text-xs font-medium text-amber-700",
				{ message }
			}
		});
	}
	if is_fetching {
		return page!({
			div {
				class: "border-b border-cloud-100 bg-cloud-50 px-4 py-2 text-xs font-medium text-cloud-600",
				"Refreshing " { label }"..."
			}
		});
	}
	Page::Empty
}

fn render_account_queries(
	user: QuerySnapshot<UserInfo, ServerFnError>,
	providers: QuerySnapshot<Vec<OAuthProviderInfo>, ServerFnError>,
	linked: QuerySnapshot<Vec<LinkedOAuthAccountInfo>, ServerFnError>,
) -> Page {
	match user.status {
		QueryStatus::Idle => {
			return account_loading("Account data is not available during server rendering.");
		}
		QueryStatus::Pending => return account_loading("Loading account..."),
		QueryStatus::Error => {
			return account_error(&query_error_message(
				user.error,
				"Account data is temporarily unavailable.",
			));
		}
		QueryStatus::Success => {}
	}

	let user_refresh_notice =
		query_refresh_notice(user.is_fetching, user.refetch_error, "account details");
	let provider_refresh_notice = query_refresh_notice(
		providers.is_fetching,
		providers.refetch_error,
		"OAuth providers",
	);
	let linked_refresh_notice =
		query_refresh_notice(linked.is_fetching, linked.refetch_error, "linked accounts");
	let Some(user) = user.data else {
		return account_error("Account data is temporarily unavailable.");
	};
	let providers = match providers.status {
		QueryStatus::Idle => {
			return account_loading(
				"Account integrations are not available during server rendering.",
			);
		}
		QueryStatus::Pending => return account_loading("Loading account integrations..."),
		QueryStatus::Error => {
			return account_error(&query_error_message(
				providers.error,
				"OAuth providers are temporarily unavailable.",
			));
		}
		QueryStatus::Success => match providers.data {
			Some(providers) => providers,
			None => return account_error("OAuth providers are temporarily unavailable."),
		},
	};
	let linked = match linked.status {
		QueryStatus::Idle => {
			return account_loading(
				"Account integrations are not available during server rendering.",
			);
		}
		QueryStatus::Pending => return account_loading("Loading account integrations..."),
		QueryStatus::Error => {
			return account_error(&query_error_message(
				linked.error,
				"Linked accounts are temporarily unavailable.",
			));
		}
		QueryStatus::Success => match linked.data {
			Some(linked) => linked,
			None => return account_error("Linked accounts are temporarily unavailable."),
		},
	};

	let notices = vec![
		user_refresh_notice,
		provider_refresh_notice,
		linked_refresh_notice,
	];
	let content = render_account_content(user, providers, linked);
	page!({
		div {
			{ notices }
			{ content }
		}
	})
}

struct AccountPageViewProps {
	user: QueryHandle<UserInfo, ServerFnError>,
	providers: QueryHandle<Vec<OAuthProviderInfo>, ServerFnError>,
	linked: QueryHandle<Vec<LinkedOAuthAccountInfo>, ServerFnError>,
}

/// Render the account page.
#[reinhardt::pages::component("account", name = "auth:account_page")]
pub fn account_page() -> Page {
	let props = AccountPageViewProps {
		user: use_query(me::query(), QueryOptions::new().enabled(cfg!(wasm))),
		providers: use_query(
			list_oauth_providers::query(),
			QueryOptions::new().enabled(cfg!(wasm)),
		),
		linked: use_query(
			list_linked_oauth_accounts::query(),
			QueryOptions::new().enabled(cfg!(wasm)),
		),
	};
	Page::reactive(move || {
		render_account_queries(
			props.user.snapshot(),
			props.providers.snapshot(),
			props.linked.snapshot(),
		)
	})
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use super::*;

	fn successful_query<T>(data: T) -> QuerySnapshot<T, ServerFnError> {
		QuerySnapshot {
			status: QueryStatus::Success,
			data: Some(data),
			error: None,
			refetch_error: None,
			is_fetching: false,
			is_stale: false,
		}
	}

	#[rstest]
	fn account_content_renders_link_action_when_github_is_unlinked() {
		// Arrange
		let user = UserInfo {
			id: "user-1".to_string(),
			username: "alice".to_string(),
			email: "alice@example.com".to_string(),
		};
		let providers = vec![OAuthProviderInfo {
			id: "github".to_string(),
			label: "GitHub".to_string(),
			start_url: "/api/auth/oauth/github/start/".to_string(),
		}];

		// Act
		let html = render_account_content(user, providers, Vec::new()).render_to_string();

		// Assert
		assert!(html.contains("Link GitHub"));
		assert!(html.contains(r#"href="/api/auth/oauth/github/start/?intent=link""#));
		assert!(html.contains(r#"rel="external""#));
	}

	#[rstest]
	fn account_content_renders_linked_state_without_link_action() {
		// Arrange
		let user = UserInfo {
			id: "user-1".to_string(),
			username: "alice".to_string(),
			email: "alice@example.com".to_string(),
		};
		let linked = vec![LinkedOAuthAccountInfo {
			provider: "github".to_string(),
			label: "GitHub".to_string(),
			provider_username: Some("octocat".to_string()),
		}];

		// Act
		let html = render_account_content(user, Vec::new(), linked).render_to_string();

		// Assert
		assert!(html.contains("GitHub account linked"));
		assert!(html.contains("octocat"));
		assert!(!html.contains("Link GitHub"));
	}

	#[rstest]
	fn account_read_queries_use_generated_server_function_families() {
		// Arrange
		let user = me::query();
		let providers = list_oauth_providers::query();
		let linked = list_linked_oauth_accounts::query();

		// Act
		let user_family = user.key().family_id();
		let provider_family = providers.key().family_id();
		let linked_family = linked.key().family_id();

		// Assert
		assert_eq!(user_family, me::family().id());
		assert_eq!(provider_family, list_oauth_providers::family().id());
		assert_eq!(linked_family, list_linked_oauth_accounts::family().id());
		assert_ne!(user_family, provider_family);
		assert_ne!(user_family, linked_family);
	}

	#[rstest]
	fn account_query_initial_states_render_loading_or_error() {
		// Arrange
		let pending = QuerySnapshot {
			status: QueryStatus::Pending,
			data: None,
			error: None,
			refetch_error: None,
			is_fetching: true,
			is_stale: false,
		};
		let failed = QuerySnapshot {
			status: QueryStatus::Error,
			data: None,
			error: Some(ServerFnError::application("Session expired")),
			refetch_error: None,
			is_fetching: false,
			is_stale: false,
		};

		// Act
		let pending_html = render_account_queries(
			pending,
			successful_query(Vec::new()),
			successful_query(Vec::new()),
		)
		.render_to_string();
		let failed_html = render_account_queries(
			failed,
			successful_query(Vec::new()),
			successful_query(Vec::new()),
		)
		.render_to_string();

		// Assert
		assert!(pending_html.contains("Loading account..."));
		assert!(failed_html.contains("Session expired"));
		assert!(failed_html.contains("Sign in"));
	}

	#[rstest]
	fn account_query_refetch_error_keeps_cached_profile_visible() {
		// Arrange
		let user = QuerySnapshot {
			status: QueryStatus::Success,
			data: Some(UserInfo {
				id: "user-1".to_string(),
				username: "alice".to_string(),
				email: "alice@example.com".to_string(),
			}),
			error: None,
			refetch_error: Some(ServerFnError::application("Refresh timed out")),
			is_fetching: false,
			is_stale: true,
		};

		// Act
		let html = render_account_queries(
			user,
			successful_query(Vec::new()),
			successful_query(Vec::new()),
		)
		.render_to_string();

		// Assert
		assert!(html.contains("alice@example.com"));
		assert!(html.contains("Showing cached account details; the latest refresh failed"));
		assert!(html.contains("Refresh timed out"));
	}
}
