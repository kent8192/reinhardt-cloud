//! WASM entry point for cloud_dashboard.
//!
//! Hydrates the initial Project surface, or mounts client routes for a CSR document.
//! Each app exposes its own client router through `apps/<app>/urls.rs`;
//! combine them in
//! `src/config/urls.rs` so the `#[routes]` registration owns both server and
//! client route aggregation.

use reinhardt::pages::hydration::{hydrate, init_hydration_state};
use reinhardt::pages::{ClientLauncher, Component, Page};
use wasm_bindgen::prelude::{JsValue, wasm_bindgen};

struct DashboardRoot;

impl Component for DashboardRoot {
	fn render(&self) -> Page {
		crate::apps::project::client::components::projects::initial_project_page()
	}

	fn name() -> &'static str {
		"CloudDashboardRoot"
	}
}

#[wasm_bindgen(start)]
pub fn main() -> Result<(), JsValue> {
	console_error_panic_hook::set_once();
	let document = web_sys::window()
		.and_then(|window| window.document())
		.ok_or_else(|| JsValue::from_str("The Dashboard document is unavailable"))?;
	let root = document
		.get_element_by_id("root")
		.ok_or_else(|| JsValue::from_str("The Dashboard root is unavailable"))?;
	if document.get_element_by_id("ssr-state").is_some() {
		reinhardt::pages::reactive::runtime::set_scheduler(|task| {
			wasm_bindgen_futures::spawn_local(async move { task() });
		});
		init_hydration_state();
		let component_root = root
			.first_element_child()
			.ok_or_else(|| JsValue::from_str("The SSR component root is unavailable"))?;
		hydrate(
			&DashboardRoot,
			&reinhardt::pages::dom::Element::new(component_root),
		)
		.map_err(|error| JsValue::from_str(&error.to_string()))?;
		root.set_attribute("data-render-mode", "hydrated")?;
	} else {
		ClientLauncher::new("#root")
			.register_routes_from_inventory()
			.launch()?;
		root.set_attribute("data-render-mode", "mounted")?;
	}
	Ok(())
}
