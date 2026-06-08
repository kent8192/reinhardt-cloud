//! Account page for profile and OAuth account linking.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{Resource, ResourceState, use_resource};

use crate::apps::auth::server_fn::linked_accounts::LinkedOAuthAccountInfo;
use crate::apps::auth::server_fn::oauth_providers::OAuthProviderInfo;
use crate::apps::dashboard::client::layout::dashboard_app_shell;
use crate::shared::UserInfo;

#[cfg(wasm)]
use crate::apps::auth::server_fn::linked_accounts::list_linked_oauth_accounts;
#[cfg(wasm)]
use crate::apps::auth::server_fn::me::me;
#[cfg(wasm)]
use crate::apps::auth::server_fn::oauth_providers::list_oauth_providers;

#[cfg(wasm)]
async fn load_current_user() -> Result<UserInfo, String> {
	me().await.map_err(|err| err.to_string())
}

#[cfg(not(wasm))]
async fn load_current_user() -> Result<UserInfo, String> {
	Err("current user is loaded by the browser client".to_string())
}

#[cfg(wasm)]
async fn load_linked_accounts() -> Result<Vec<LinkedOAuthAccountInfo>, String> {
	list_linked_oauth_accounts()
		.await
		.map_err(|err| err.to_string())
}

#[cfg(not(wasm))]
async fn load_linked_accounts() -> Result<Vec<LinkedOAuthAccountInfo>, String> {
	Ok(Vec::new())
}

#[cfg(wasm)]
async fn load_oauth_providers() -> Result<Vec<OAuthProviderInfo>, String> {
	list_oauth_providers().await.map_err(|err| err.to_string())
}

#[cfg(not(wasm))]
async fn load_oauth_providers() -> Result<Vec<OAuthProviderInfo>, String> {
	Ok(Vec::new())
}

fn github_link_url(providers: &[OAuthProviderInfo]) -> Option<String> {
	providers
		.iter()
		.find(|provider| provider.id == "github")
		.map(|provider| provider.start_url.clone())
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
	page!(|user: UserInfo, github_linked: bool, github_label: String, github_link_url: Option<String>| {
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
								{
									user.username.clone()
								}
							}
						}
						div {
							dt {
								class: "font-medium text-ink-600",
								"Email"
							}
							dd {
								class: "mt-1 text-ink-950",
								{
									user.email.clone()
								}
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
						}
						{
							if github_linked {
								page!(|label: String| {
									span {
										class: "inline-flex shrink-0 rounded-full bg-control-500/10 px-2.5 py-1 text-xs font-semibold text-control-700",
										{ label }
									}
								})(github_label.clone())
							} else { Page::Empty }
						}
					}
					div {
						class: "mt-5",
						{
							if github_linked {
								page!(|| {
									p {
										class: "text-sm font-medium text-ink-700",
										"GitHub account linked"
									}
								})()
							} else if let Some(url) = github_link_url.clone() {
								page!(|url: String| {
									a {
										href: url,
										rel: "external",
										class: "btn-primary inline-flex px-4 py-2 text-sm",
										"Link GitHub"
									}
								})(url)
							} else {
								page!(|| {
									p {
										class: "text-sm font-medium text-ink-600",
										"GitHub OAuth is not configured"
									}
								})()
							}
						}
					}
				}
			}
		}
	})(user, github_linked, github_label, github_link_url)
}

fn account_error(message: &str) -> Page {
	let login_href = "/login".to_string();
	page!(|message: String, login_href: String| {
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
	})(message.to_string(), login_href)
}

/// Render the account page.
pub fn account_page() -> Page {
	let user = use_resource(|| async move { self::load_current_user().await }, ());
	let providers = use_resource(|| async move { self::load_oauth_providers().await }, ());
	let linked = use_resource(|| async move { self::load_linked_accounts().await }, ());

	let content =
		page!(|user: Resource<UserInfo, String>,
		       providers: Resource<Vec<OAuthProviderInfo>, String>,
		       linked: Resource<Vec<LinkedOAuthAccountInfo>, String>| {
			{
				match (user.get(), providers.get(), linked.get()) {
					(
						ResourceState::Success(user),
						ResourceState::Success(providers),
						ResourceState::Success(linked),
					) => self::render_account_content(user, providers, linked),
					(ResourceState::Error(err), _, _) => self::account_error(&err),
					(_, ResourceState::Error(err), _) => self::account_error(&err),
					(_, _, ResourceState::Error(err)) => self::account_error(&err),
					_ => Page::Empty,
				}
			}
		})(user, providers, linked);

	dashboard_app_shell("account", content)
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use super::*;

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
		assert!(html.contains(r#"href="/api/auth/oauth/github/start/""#));
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
}
