//! OAuth provider buttons for auth pages.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{Resource, ResourceState, use_resource};

use crate::apps::auth::server_fn::oauth_providers::OAuthProviderInfo;
#[cfg(wasm)]
use crate::apps::auth::server_fn::oauth_providers::list_oauth_providers;

#[cfg(wasm)]
async fn load_oauth_providers() -> Result<Vec<OAuthProviderInfo>, String> {
	list_oauth_providers().await.map_err(|err| err.to_string())
}

#[cfg(not(wasm))]
async fn load_oauth_providers() -> Result<Vec<OAuthProviderInfo>, String> {
	Ok(Vec::new())
}

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
				{
					providers.clone().into_iter().map(|provider| {
						page!(|href: String, label: String| {
							a {
								href: href,
								class: "inline-flex w-full items-center justify-center rounded-md border border-cloud-200 bg-white px-4 py-2.5 text-sm font-semibold text-ink-800 shadow-sm transition hover:bg-cloud-50 focus:outline-none focus:ring-2 focus:ring-control-500 focus:ring-offset-2",
								{ label }
							}
						})(provider.start_url, provider.label)
					}).collect::<Vec<_>>()
				}
			}
		}
	})(providers)
}

/// Render OAuth provider buttons when providers are configured.
pub fn oauth_buttons() -> Page {
	let providers = use_resource(|| async move { self::load_oauth_providers().await }, ());

	page!(|providers: Resource<Vec<OAuthProviderInfo>, String>| {
		{
			match providers.get() {
				ResourceState::Loading | ResourceState::Error(_) => Page::Empty,
				ResourceState::Success(items) => self::render_provider_buttons(items),
			}
		}
	})(providers)
}
