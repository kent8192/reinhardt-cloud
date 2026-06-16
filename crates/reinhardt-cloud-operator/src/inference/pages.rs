//! Pages inference: detection and configuration resolution for reinhardt-pages.

use std::collections::BTreeMap;

use reinhardt_cloud_types::crd::Project;
use reinhardt_cloud_types::crd::pages::PagesResourceRequirements;

/// Resolved pages configuration with all defaults applied.
///
/// Computed from `PagesSpec` (explicit) or introspect signals (auto-detected).
/// Passed to resource builders (deployment, service, ingress).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedPagesConfig {
	pub static_root: String,
	pub static_url: String,
	pub server_image: String,
	pub server_resources: PagesResourceRequirements,
	pub cache_max_age: u64,
	pub brotli: bool,
	pub gzip: bool,
}

impl Default for ResolvedPagesConfig {
	fn default() -> Self {
		Self {
			static_root: "/app/staticfiles".to_string(),
			static_url: "/static/".to_string(),
			server_image: "joseluisq/static-web-server:2-alpine".to_string(),
			server_resources: PagesResourceRequirements {
				requests: BTreeMap::from([
					("cpu".to_string(), "10m".to_string()),
					("memory".to_string(), "16Mi".to_string()),
				]),
				limits: BTreeMap::from([
					("cpu".to_string(), "100m".to_string()),
					("memory".to_string(), "64Mi".to_string()),
				]),
			},
			cache_max_age: 86400,
			brotli: true,
			gzip: true,
		}
	}
}

/// Determines whether pages deployment should be enabled.
///
/// Priority: explicit `spec.pages` (Some = enabled) > introspect signals > false.
pub(crate) fn should_enable_pages(app: &Project) -> bool {
	if app.spec.pages.is_some() {
		return true;
	}
	app.spec
		.introspect
		.as_ref()
		.map(|i| i.features.infrastructure_signals.pages)
		.unwrap_or(false)
}

/// Resolves pages configuration by merging explicit spec over defaults.
///
/// Returns `None` if pages is not enabled.
pub(crate) fn resolve_pages_config(app: &Project) -> Option<ResolvedPagesConfig> {
	if !should_enable_pages(app) {
		return None;
	}

	let defaults = ResolvedPagesConfig::default();

	let config = match &app.spec.pages {
		Some(pages) => ResolvedPagesConfig {
			static_root: pages.static_root.clone().unwrap_or(defaults.static_root),
			static_url: pages.static_url.clone().unwrap_or(defaults.static_url),
			server_image: pages.server_image.clone().unwrap_or(defaults.server_image),
			server_resources: pages
				.server_resources
				.clone()
				.unwrap_or(defaults.server_resources),
			cache_max_age: pages.cache_max_age.unwrap_or(defaults.cache_max_age),
			brotli: pages.brotli.unwrap_or(defaults.brotli),
			gzip: pages.gzip.unwrap_or(defaults.gzip),
		},
		None => defaults,
	};

	Some(config)
}

#[cfg(test)]
mod tests {
	use super::*;
	use kube::api::ObjectMeta;
	use reinhardt_cloud_types::crd::ProjectSpec;
	use reinhardt_cloud_types::crd::pages::PagesSpec;
	use reinhardt_cloud_types::introspect::{FeaturesMetadata, InfraSignals, IntrospectOutput};
	use rstest::rstest;

	fn make_test_app(name: &str) -> Project {
		Project {
			metadata: ObjectMeta {
				name: Some(name.to_string()),
				namespace: Some("default".to_string()),
				uid: Some("test-uid-12345".to_string()),
				..Default::default()
			},
			spec: ProjectSpec {
				image: "img:v1".to_string(),
				..Default::default()
			},
			status: None,
		}
	}

	fn make_introspect_with_pages(pages: bool) -> IntrospectOutput {
		IntrospectOutput {
			features: FeaturesMetadata {
				infrastructure_signals: InfraSignals {
					pages,
					..Default::default()
				},
				..Default::default()
			},
			..Default::default()
		}
	}

	#[rstest]
	fn test_should_enable_pages_explicit_some() {
		// Arrange
		let mut app = make_test_app("app");
		app.spec.pages = Some(PagesSpec {
			static_root: None,
			static_url: None,
			server_image: None,
			server_resources: None,
			cache_max_age: None,
			brotli: None,
			gzip: None,
		});

		// Act / Assert
		assert!(should_enable_pages(&app));
	}

	#[rstest]
	fn test_should_enable_pages_explicit_none_no_introspect() {
		// Arrange
		let app = make_test_app("app");

		// Act / Assert
		assert!(!should_enable_pages(&app));
	}

	#[rstest]
	fn test_should_enable_pages_from_introspect() {
		// Arrange
		let mut app = make_test_app("app");
		app.spec.introspect = Some(make_introspect_with_pages(true));

		// Act / Assert
		assert!(should_enable_pages(&app));
	}

	#[rstest]
	fn test_should_enable_pages_introspect_false() {
		// Arrange
		let mut app = make_test_app("app");
		app.spec.introspect = Some(make_introspect_with_pages(false));

		// Act / Assert
		assert!(!should_enable_pages(&app));
	}

	#[rstest]
	fn test_resolve_pages_config_none_when_disabled() {
		// Arrange
		let app = make_test_app("app");

		// Act
		let config = resolve_pages_config(&app);

		// Assert
		assert!(config.is_none());
	}

	#[rstest]
	fn test_resolve_pages_config_defaults_from_introspect() {
		// Arrange
		let mut app = make_test_app("app");
		app.spec.introspect = Some(make_introspect_with_pages(true));

		// Act
		let config = resolve_pages_config(&app).unwrap();

		// Assert
		assert_eq!(config, ResolvedPagesConfig::default());
	}

	#[rstest]
	fn test_resolve_pages_config_custom_static_url() {
		// Arrange
		let mut app = make_test_app("app");
		app.spec.pages = Some(PagesSpec {
			static_root: None,
			static_url: Some("/assets/".to_string()),
			server_image: None,
			server_resources: None,
			cache_max_age: None,
			brotli: None,
			gzip: None,
		});

		// Act
		let config = resolve_pages_config(&app).unwrap();

		// Assert
		assert_eq!(config.static_url, "/assets/");
		assert_eq!(config.static_root, "/app/staticfiles");
	}

	#[rstest]
	fn test_resolve_pages_config_custom_overrides() {
		// Arrange
		let mut app = make_test_app("app");
		app.spec.pages = Some(PagesSpec {
			static_root: Some("/opt/static".to_string()),
			static_url: Some("/assets/".to_string()),
			server_image: None,
			server_resources: None,
			cache_max_age: Some(604800),
			brotli: Some(false),
			gzip: None,
		});

		// Act
		let config = resolve_pages_config(&app).unwrap();

		// Assert
		assert_eq!(config.static_root, "/opt/static");
		assert_eq!(config.static_url, "/assets/");
		assert_eq!(config.server_image, "joseluisq/static-web-server:2-alpine");
		assert_eq!(config.cache_max_age, 604800);
		assert!(!config.brotli);
		assert!(config.gzip);
	}
}
