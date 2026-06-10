//! OAuth provider sign-in buttons.
//!
//! Renders one anchor per enabled provider. Each button is a plain link to
//! `/api/auth/oauth/{provider}/start/` so the browser performs a normal
//! navigation, the server issues a 302 to the provider's authorize URL,
//! and the user lands back on `/api/auth/oauth/{provider}/callback/`.

#[cfg(wasm)]
use crate::apps::auth::server::oauth_providers::OAuthProviderInfo;
#[cfg(wasm)]
use crate::apps::auth::server::oauth_providers::list_oauth_providers;

/// Populate an OAuth provider mount point from server-side configuration.
#[cfg(wasm)]
pub fn ensure_oauth_buttons_connected(container_id: &'static str, verb: &'static str) {
	wasm_bindgen_futures::spawn_local(async move {
		let Ok(providers) = list_oauth_providers().await else {
			return;
		};
		if providers.is_empty() {
			return;
		}
		render_oauth_buttons(container_id, verb, providers);
	});
}

/// Non-WASM stub so native builds can share the same client entry wiring.
#[cfg(not(wasm))]
#[allow(dead_code)] // Called from WASM route wiring only; native builds keep the shared API surface.
pub fn ensure_oauth_buttons_connected(_container_id: &'static str, _verb: &'static str) {}

#[cfg(wasm)]
fn render_oauth_buttons(container_id: &str, verb: &str, providers: Vec<OAuthProviderInfo>) {
	let Some(document) = web_sys::window().and_then(|window| window.document()) else {
		return;
	};
	let Some(container) = document.get_element_by_id(container_id) else {
		return;
	};
	container.set_inner_html("");

	let Ok(wrapper) = document.create_element("div") else {
		return;
	};
	let _ = wrapper.set_attribute("class", "mt-6");

	let Ok(divider) = document.create_element("div") else {
		return;
	};
	let _ = divider.set_attribute("class", "relative");
	divider.set_inner_html(
		r#"<div class="absolute inset-0 flex items-center"><span class="w-full border-t border-cloud-200"></span></div><div class="relative flex justify-center text-xs uppercase"><span class="bg-white px-2 text-ink-600">Or continue with</span></div>"#,
	);
	let _ = wrapper.append_child(&divider);

	let Ok(grid) = document.create_element("div") else {
		return;
	};
	let _ = grid.set_attribute("class", "mt-4 grid grid-cols-1 gap-2");

	for provider in providers {
		let Ok(anchor) = document.create_element("a") else {
			continue;
		};
		let Some(href) = crate::apps::auth::client::routes::oauth_start_path(&provider.id) else {
			continue;
		};
		let _ = anchor.set_attribute("href", &href);
		let _ = anchor.set_attribute(
			"class",
			"inline-flex w-full items-center justify-center rounded-md border border-cloud-200 py-2.5 text-sm font-semibold text-ink-800 transition hover:bg-cloud-50",
		);
		anchor.set_text_content(Some(&format!("{verb} with {}", provider.label)));
		let _ = grid.append_child(&anchor);
	}

	let _ = wrapper.append_child(&grid);
	let _ = container.append_child(&wrapper);
}
