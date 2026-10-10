//! Static artifact registration for `collectstatic`.
//!
//! Registers the `dist-wasm` directory containing WASM build artifacts and the
//! `static` directory containing design tokens, base styles, and images so
//! `cargo run --bin manage collectstatic` discovers and copies them into the
//! final distribution directory.

use reinhardt::reinhardt_apps::AppStaticFilesConfig;

inventory::submit! {
	AppStaticFilesConfig {
		app_label: "cloud_control_plane-wasm",
		static_dir: "dist-wasm",
		url_prefix: "",
	}
}

// Design tokens, base styles, utilities, and images that component-scoped
// `style!` definitions cannot express. `collectstatic` publishes the
// directory beside the WASM bundle, so `static_url("css/tokens.css")` resolves
// in `index.html`.
inventory::submit! {
	AppStaticFilesConfig {
		app_label: "cloud_control_plane-static",
		static_dir: "static",
		url_prefix: "",
	}
}
