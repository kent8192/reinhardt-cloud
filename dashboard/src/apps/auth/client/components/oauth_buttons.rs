//! OAuth provider buttons for auth pages.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{QueryHandle, QueryOptions, QueryStatus, use_query};
use reinhardt::pages::server_fn::ServerFnError;

use crate::apps::auth::server_fn::oauth_providers::{OAuthProviderInfo, list_oauth_providers};

fn render_provider_buttons(providers: Vec<OAuthProviderInfo>) -> Page {
	if providers.is_empty() {
		return Page::Empty;
	}

	page!(|providers: Vec<OAuthProviderInfo>| {
		div {
			class: "mt-6 space-y-4",
			div {
				class: "relative",
				div {
					class: "absolute inset-0 flex items-center",
					div {
						class: "w-full border-t border-cloud-200",
					}
				}
				div {
					class: "relative flex justify-center text-sm",
					span {
						class: "bg-white px-2 text-ink-500",
						"Or continue with"
					}
				}
			}
			div {
				class: "grid gap-2",
				{ providers.clone().into_iter().map(|provider| {
					page!(|href: String, label: String| {
						a {
							href: href,
							rel: "external",
							class: "inline-flex w-full items-center justify-center rounded-md border border-cloud-200 bg-white px-4 py-2.5 text-sm font-semibold text-ink-800 shadow-sm transition hover:bg-cloud-50 focus:outline-none focus:ring-2 focus:ring-control-500 focus:ring-offset-2",
							{ label }
						}
					})(provider.start_url, provider.label)
				}).collect::<Vec<_>>() }
			}
		}
	})(providers)
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
		QueryStatus::Pending => page!(|| {
			p {
				class: "mt-4 text-center text-xs font-medium text-ink-500",
				"Loading sign-in options..."
			}
		})(),
		QueryStatus::Error => page!(|message: String| {
			p {
				class: "mt-4 text-center text-xs font-medium text-red-700",
				{ message }
			}
		})(
			snapshot
				.error
				.map(|error| error.user_message().to_string())
				.unwrap_or_else(|| "OAuth sign-in is unavailable.".to_string()),
		),
		QueryStatus::Success => {
			let buttons = render_provider_buttons(snapshot.data.unwrap_or_default());
			let notice = if let Some(error) = snapshot.refetch_error {
				page!(|message: String| {
					p {
						class: "mt-4 text-center text-xs font-medium text-amber-700",
						{ message }
					}
				})(format!(
					"Showing cached sign-in options; refresh failed: {}",
					error.user_message()
				))
			} else if snapshot.is_fetching {
				page!(|| {
					p {
						class: "mt-4 text-center text-xs font-medium text-ink-500",
						"Refreshing sign-in options..."
					}
				})()
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
