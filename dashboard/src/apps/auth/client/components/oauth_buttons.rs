//! OAuth provider sign-in buttons.
//!
//! Renders one anchor per enabled provider. Each button is a plain link to
//! `/api/auth/oauth/{provider}/start/` so the browser performs a normal
//! navigation, the server issues a 302 to the provider's authorize URL,
//! and the user lands back on `/api/auth/oauth/{provider}/callback/`.

use reinhardt::pages::component::Page;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{ResourceState, use_resource};

use crate::apps::auth::server::oauth_providers::OAuthProviderInfo;
#[cfg(wasm)]
use crate::apps::auth::server::oauth_providers::list_oauth_providers;

#[cfg(wasm)]
async fn load_oauth_providers() -> Result<Vec<OAuthProviderInfo>, String> {
	list_oauth_providers().await.map_err(|e| e.to_string())
}

#[cfg(not(wasm))]
async fn load_oauth_providers() -> Result<Vec<OAuthProviderInfo>, String> {
	Ok(Vec::new())
}

/// Render the OAuth provider button group below the password form.
///
/// `verb` is the leading word ("Sign in" on login, "Sign up" on register)
/// so the button reads naturally in context.
pub fn oauth_buttons(verb: &'static str) -> Page {
	let providers = use_resource(|| async move { self::load_oauth_providers().await }, ());

	page!(|verb: &'static str,
	       providers: reinhardt::pages::prelude::Resource<Vec<OAuthProviderInfo>, String>| {
		{
			match providers.get() {
				ResourceState::Success(providers) if !providers.is_empty() => {
					page!(|verb: &'static str, providers: Vec<OAuthProviderInfo>| {
						div {
							class: "mt-6",
							div {
								class: "relative",
								div {
									class: "absolute inset-0 flex items-center",
									span {
										class: "w-full border-t border-gray-200",
									}
								}
								div {
									class: "relative flex justify-center text-xs uppercase",
									span {
										class: "bg-white px-2 text-gray-500",
										"Or continue with"
									}
								}
							}
							div {
								class: "mt-4 grid grid-cols-1 gap-2",
								{
									providers
										.clone()
										.into_iter()
										.map(|provider| {
											page!(|verb: &'static str, provider: OAuthProviderInfo| {
												a {
													href: format!("/api/auth/oauth/{}/start/", provider.id),
													class: "inline-flex items-center justify-center w-full py-2.5 text-sm font-medium border border-gray-300 rounded-md hover:bg-gray-50",
													{
														format!("{verb} with {}", provider.label)
													}
												}
											})(verb, provider)
										})
										.collect::<Vec<_>>()
								}
							}
						}
					})(verb, providers)
				}
				_ => Page::Empty,
			}
		}
	})(verb, providers)
}
