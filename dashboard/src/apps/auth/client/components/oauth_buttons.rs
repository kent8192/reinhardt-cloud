//! OAuth provider buttons for auth pages.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{QueryHandle, QueryOptions, QueryStatus, use_query};
use reinhardt::pages::server_fn::ServerFnError;

use crate::apps::auth::client::style::STYLES;
use crate::apps::auth::server_fn::oauth_providers::{OAuthProviderInfo, list_oauth_providers};
use crate::shared::client::style::STYLES as SHARED_STYLES;

fn render_provider_buttons(providers: Vec<OAuthProviderInfo>) -> Page {
	if providers.is_empty() {
		return Page::Empty;
	}

	page!({
		div {
			class: STYLES.oauth_section(),
			div {
				class: STYLES.oauth_divider(),
				div {
					class: STYLES.oauth_divider_line(),
				}
				span { "Or continue with" }
				div {
					class: STYLES.oauth_divider_line(),
				}
			}
			div {
				class: STYLES.oauth_options(),
				{ providers
				.clone()
				.into_iter()
				.map(|provider| {
					page!({
						a {
							href: provider.start_url,
							rel: "external",
							class: SHARED_STYLES.button_secondary() + STYLES.oauth_button(),
							{ provider.label }
						}
					})
				})
				.collect::<Vec<_>>() }
			}
		}
	})
}

/// Render OAuth provider buttons when providers are configured.
pub fn oauth_buttons() -> Page {
	let providers = use_query(
		list_oauth_providers::query(),
		QueryOptions::new().enabled(cfg!(wasm)),
	);
	Page::reactive(move || render_provider_query(&providers))
}

fn render_provider_query(providers: &QueryHandle<Vec<OAuthProviderInfo>, ServerFnError>) -> Page {
	let snapshot = providers.snapshot();
	match snapshot.status {
		QueryStatus::Idle => Page::Empty,
		QueryStatus::Pending => page!({
			p {
				class: STYLES.oauth_status(),
				"Loading sign-in options..."
			}
		}),
		QueryStatus::Error => {
			let message = snapshot
				.error
				.map(|error| error.user_message().to_string())
				.unwrap_or_else(|| "OAuth sign-in is unavailable.".to_string());
			page!({
				p {
					class: STYLES.oauth_status() + STYLES.oauth_error(),
					{ message }
				}
			})
		}
		QueryStatus::Success => {
			let buttons = render_provider_buttons(snapshot.data.unwrap_or_default());
			let notice = if let Some(error) = snapshot.refetch_error {
				let message = format!(
					"Showing cached sign-in options; refresh failed: {}",
					error.user_message()
				);
				page!({
					p {
						class: STYLES.oauth_status() + STYLES.oauth_warning(),
						{ message }
					}
				})
			} else if snapshot.is_fetching {
				page!({
					p {
						class: STYLES.oauth_status(),
						"Refreshing sign-in options..."
					}
				})
			} else {
				Page::Empty
			};
			Page::fragment([notice, buttons])
		}
	}
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use super::*;

	#[rstest]
	fn provider_buttons_mark_oauth_start_links_as_external() {
		// Arrange
		let providers = vec![OAuthProviderInfo {
			id: "github".to_string(),
			label: "GitHub".to_string(),
			start_url: "/api/auth/oauth/github/start/".to_string(),
		}];

		// Act
		let html = render_provider_buttons(providers).render_to_string();

		// Assert
		assert!(
			html.contains(r#"href="/api/auth/oauth/github/start/""#),
			"OAuth provider link should use the server-generated start URL: {html}"
		);
		assert!(
			html.contains(r#"rel="external""#),
			"OAuth provider links must bypass the SPA link interceptor: {html}"
		);
	}
}
