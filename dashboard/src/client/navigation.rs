//! Named links preserve SSR fallback and activate the Pages router on navigation.
use crate::client::components::shell::STYLES;
use reinhardt::pages::{Page, page};
pub fn projects_path(organization_id: uuid::Uuid) -> String {
	crate::config::urls::client_routes()
		.reverse(
			"project-list",
			&[("organization_id", &organization_id.to_string())],
		)
		.expect("registered Project list route")
}
pub fn detail_path(organization_id: uuid::Uuid, project_id: uuid::Uuid) -> String {
	crate::config::urls::client_routes()
		.reverse(
			"project-detail",
			&[
				("organization_id", &organization_id.to_string()),
				("project_id", &project_id.to_string()),
			],
		)
		.expect("registered Project detail route")
}
pub fn route_link(path: String, label: String) -> Page {
	let destination = path.clone();
	page!({
		a {
			href: path,
			class: STYLES.link(),
			@click: move |event: reinhardt::pages::event::ClickEvent| {
				handle_link(event, &destination);
			},
			{ label }
		}
	})
}
#[cfg(wasm)]
fn navigate(path: &str) {
	if reinhardt::pages::app::try_with_spa_router(|_| ()).is_some() {
		if let Err(error) = reinhardt::pages::navigate(path, reinhardt::pages::NavigationType::Push)
		{
			reinhardt::pages::error_log!("Dashboard navigation failed: {error}");
		}
		return;
	}
	// Hydration adopts the initial DOM. ClientLauncher owns subsequent route transitions.
	let destination = path.to_owned();
	// Launch after the hydrated event scope exits so the new i18n context owns its lifetime.
	wasm_bindgen_futures::spawn_local(async move {
		let path = destination.as_str();
		let result = (|| {
			let window = web_sys::window().ok_or(wasm_bindgen::JsValue::NULL)?;
			window
				.history()?
				.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path))?;
			let locale = window
				.document()
				.and_then(|document| document.document_element())
				.and_then(|html| html.get_attribute("lang"))
				.unwrap_or_else(|| "en".into());
			reinhardt::pages::ClientLauncher::new("#root")
				.i18n_context(crate::client::components::shell::translations(&locale))
				.register_routes_from_inventory()
				.launch()?;
			if let Some(root) = window.document().and_then(|d| d.get_element_by_id("root")) {
				root.set_attribute("data-render-mode", "routed")?;
			}
			Ok::<(), wasm_bindgen::JsValue>(())
		})();
		if result.is_err() {
			if let Some(window) = web_sys::window() {
				let _ = window.location().set_href(path);
			}
		}
	});
}

fn handle_link(event: reinhardt::pages::event::ClickEvent, destination: &str) {
	#[cfg(wasm)]
	{
		let modifiers = event.modifiers();
		if event.button() == reinhardt::pages::event::MouseButton::Primary
			&& !modifiers.alt
			&& !modifiers.control
			&& !modifiers.meta
			&& !modifiers.shift
		{
			event.prevent_default();
			event.stop_propagation();
			navigate(destination);
		}
	}
	#[cfg(server)]
	let _ = (event, destination);
}
