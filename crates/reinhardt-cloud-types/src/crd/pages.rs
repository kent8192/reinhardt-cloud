//! Configuration types for reinhardt-pages (WASM frontend) deployment.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::validation::ValidationError;

/// reinhardt-pages frontend deployment configuration.
///
/// Presence of `Some(PagesSpec)` in `ProjectSpec.pages` implicitly enables
/// pages deployment. There is no separate `enabled` field --- use `None` to disable.
/// This matches the pattern of `DatabaseSpec`, `CacheSpec`, etc.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Default)]
pub struct PagesSpec {
	/// Path inside the container where collectstatic outputs files.
	/// Defaults to `/app/staticfiles` (standard STATIC_ROOT).
	pub static_root: Option<String>,
	/// URL prefix for static files. Defaults to `/static/`.
	/// Must start and end with `/`.
	pub static_url: Option<String>,
	/// static-web-server container image.
	/// Defaults to `joseluisq/static-web-server:2-alpine`.
	pub server_image: Option<String>,
	/// Resource requests/limits for the static-web-server sidecar.
	pub server_resources: Option<PagesResourceRequirements>,
	/// Cache-Control max-age for static assets (seconds).
	/// Defaults to 86400 (1 day).
	pub cache_max_age: Option<u64>,
	/// Enable Brotli compression. Defaults to true.
	pub brotli: Option<bool>,
	/// Enable Gzip compression. Defaults to true.
	pub gzip: Option<bool>,
	/// Whether the image already contains a complete `buildstatic` publication
	/// at `static_root`. When true, the operator copies that publication into
	/// the static-server volume with `cp` instead of running `collectstatic`,
	/// so the image must provide `cp` on `PATH`, and `static_root` must be a
	/// dedicated directory below `/app` (for example `/app/static`). `env`
	/// must not select another settings profile, static root, static URL, or
	/// base directory. `reinhardt-cloud init` and `sync` set this for
	/// generated Pages Dockerfiles. Defaults to false.
	pub prebuilt: Option<bool>,
}

/// CRD-safe resource requirements for the static-web-server sidecar.
///
/// Uses `BTreeMap<String, String>` instead of
/// `k8s_openapi::api::core::v1::ResourceRequirements` because the k8s-openapi
/// type may not derive `JsonSchema` consistently. Converted to k8s-openapi
/// types in the resource builder layer.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, JsonSchema, Default)]
pub struct PagesResourceRequirements {
	/// Resource requests (e.g., `{"cpu": "10m", "memory": "16Mi"}`)
	#[serde(default)]
	pub requests: BTreeMap<String, String>,
	/// Resource limits (e.g., `{"cpu": "100m", "memory": "64Mi"}`)
	#[serde(default)]
	pub limits: BTreeMap<String, String>,
}

impl PagesSpec {
	/// Validates the pages specification.
	///
	/// Checks that optional string fields are non-empty when set and
	/// that `static_url` starts and ends with `/`.
	pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
		let mut errors = Vec::new();
		if let Some(ref root) = self.static_root
			&& root.is_empty()
		{
			errors.push(ValidationError::new("pages.static_root must not be empty"));
		}
		if let Some(ref url) = self.static_url
			&& (!url.starts_with('/') || !url.ends_with('/'))
		{
			errors.push(ValidationError::new(
				"pages.static_url must start and end with '/'",
			));
		}
		if let Some(ref img) = self.server_image
			&& img.is_empty()
		{
			errors.push(ValidationError::new("pages.server_image must not be empty"));
		}
		if errors.is_empty() {
			Ok(())
		} else {
			Err(errors)
		}
	}
}

/// Settings environment variables that change where a running application
/// reads its static publication. Keys are matched case-insensitively because
/// the settings loader lowercases environment keys.
const PUBLICATION_ROOT_ENV: &[&str] = &[
	"REINHARDT_STATIC_FILES__ROOT",
	"REINHARDT_STATIC__ROOT",
	"REINHARDT_STATIC_ROOT",
];
const PUBLICATION_URL_ENV: &[&str] = &[
	"REINHARDT_STATIC_FILES__URL",
	"REINHARDT_STATIC__URL",
	"REINHARDT_STATIC_URL",
];
const PUBLICATION_BASE_DIR_ENV: &[&str] = &["REINHARDT_CORE__BASE_DIR", "REINHARDT_BASE_DIR"];

/// Returns the environment keys that would make the running application read
/// a different publication than a prebuilt Pages image ships.
///
/// A prebuilt publication is recorded from the production settings profile at
/// `static_root` and `static_url`. Application environment variables take
/// precedence over image settings, so a different profile, static root, static
/// URL, or base directory would make the app resolve assets the static-server
/// sidecar does not serve. Values equal to the recorded contract are allowed.
/// Prebuilt publications are generated with the working-directory base
/// (`base_dir = "."`), so only base directory overrides that resolve to the
/// working directory itself keep relative roots in place.
pub fn conflicting_publication_env(
	static_root: &str,
	static_url: &str,
	env: &BTreeMap<String, String>,
) -> Vec<String> {
	env.iter()
		.filter(|(key, value)| {
			let key = key.to_ascii_uppercase();
			let key = key.as_str();
			(key == "REINHARDT_ENV" && value.as_str() != "production")
				|| (PUBLICATION_ROOT_ENV.contains(&key) && value.as_str() != static_root)
				|| (PUBLICATION_URL_ENV.contains(&key) && value.as_str() != static_url)
				|| (PUBLICATION_BASE_DIR_ENV.contains(&key) && !is_working_directory(value))
		})
		.map(|(key, _)| key.clone())
		.collect()
}

/// Whether a base directory value is the working directory (`.`, `./`, `././`).
fn is_working_directory(value: &str) -> bool {
	!value.is_empty()
		&& value.split('/').all(|part| matches!(part, "." | ""))
		&& value.starts_with('.')
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_pages_spec_validate_valid() {
		// Arrange
		let spec = PagesSpec {
			static_root: Some("/app/static".to_string()),
			static_url: Some("/static/".to_string()),
			server_image: Some("joseluisq/static-web-server:2-alpine".to_string()),
			server_resources: None,
			cache_max_age: Some(86400),
			brotli: Some(true),
			gzip: Some(true),
			prebuilt: Some(true),
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn test_pages_spec_validate_all_none() {
		// Arrange
		let spec = PagesSpec {
			static_root: None,
			static_url: None,
			server_image: None,
			server_resources: None,
			cache_max_age: None,
			brotli: None,
			gzip: None,
			prebuilt: None,
		};

		// Act
		let result = spec.validate();

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn test_pages_spec_validate_empty_static_root() {
		// Arrange
		let spec = PagesSpec {
			static_root: Some(String::new()),
			static_url: None,
			server_image: None,
			server_resources: None,
			cache_max_age: None,
			brotli: None,
			gzip: None,
			prebuilt: None,
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert!(errors[0].message.contains("static_root"));
	}

	#[rstest]
	fn test_pages_spec_validate_invalid_static_url() {
		// Arrange
		let spec = PagesSpec {
			static_root: None,
			static_url: Some("static".to_string()),
			server_image: None,
			server_resources: None,
			cache_max_age: None,
			brotli: None,
			gzip: None,
			prebuilt: None,
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert!(errors[0].message.contains("static_url"));
	}

	#[rstest]
	fn test_pages_spec_validate_empty_server_image() {
		// Arrange
		let spec = PagesSpec {
			static_root: None,
			static_url: None,
			server_image: Some(String::new()),
			server_resources: None,
			cache_max_age: None,
			brotli: None,
			gzip: None,
			prebuilt: None,
		};

		// Act
		let result = spec.validate();

		// Assert
		let errors = result.unwrap_err();
		assert_eq!(errors.len(), 1);
		assert!(errors[0].message.contains("server_image"));
	}

	#[rstest]
	fn test_pages_spec_deserialization() {
		// Arrange
		let yaml = r#"
static_root: /app/dist
static_url: /assets/
server_image: custom:v1
cache_max_age: 604800
brotli: false
gzip: true
"#;

		// Act
		let spec: PagesSpec = serde_yaml::from_str(yaml).unwrap();

		// Assert
		assert_eq!(spec.static_root.unwrap(), "/app/dist");
		assert_eq!(spec.static_url.unwrap(), "/assets/");
		assert_eq!(spec.server_image.unwrap(), "custom:v1");
		assert_eq!(spec.cache_max_age.unwrap(), 604800);
		assert!(!spec.brotli.unwrap());
		assert!(spec.gzip.unwrap());
	}

	#[rstest]
	fn test_pages_spec_deserialization_empty_object() {
		// Arrange
		let yaml = "{}";

		// Act
		let spec: PagesSpec = serde_yaml::from_str(yaml).unwrap();

		// Assert
		assert!(spec.static_root.is_none());
		assert!(spec.static_url.is_none());
		assert!(spec.server_image.is_none());
		assert!(spec.server_resources.is_none());
		assert!(spec.cache_max_age.is_none());
		assert!(spec.brotli.is_none());
		assert!(spec.gzip.is_none());
	}

	#[rstest]
	fn test_pages_resource_requirements_default() {
		// Arrange / Act
		let reqs = PagesResourceRequirements::default();

		// Assert
		assert!(reqs.requests.is_empty());
		assert!(reqs.limits.is_empty());
	}

	#[rstest]
	#[case("REINHARDT_EMAIL__HOST", "smtp", false)]
	#[case("REINHARDT_ENV", "production", false)]
	#[case("REINHARDT_ENV", "staging", true)]
	#[case("reinhardt_env", "staging", true)]
	#[case("REINHARDT_STATIC_FILES__ROOT", "/app/static", false)]
	#[case("REINHARDT_STATIC_FILES__ROOT", "/app/other", true)]
	#[case("REINHARDT_STATIC__ROOT", "static", true)]
	#[case("REINHARDT_STATIC_ROOT", "/app/other", true)]
	#[case("REINHARDT_STATIC_FILES__URL", "/static/", false)]
	#[case("REINHARDT_STATIC_FILES__URL", "/assets/", true)]
	#[case("REINHARDT_STATIC__URL", "/assets/", true)]
	#[case("REINHARDT_STATIC_URL", "/assets/", true)]
	#[case("REINHARDT_CORE__BASE_DIR", "/srv", true)]
	#[case("REINHARDT_BASE_DIR", ".", false)]
	#[case("REINHARDT_BASE_DIR", "./", false)]
	#[case("REINHARDT_CORE__BASE_DIR", "././", false)]
	#[case("REINHARDT_BASE_DIR", "", true)]
	#[case("REINHARDT_BASE_DIR", "..", true)]
	#[case("REINHARDT_BASE_DIR", "./app", true)]
	#[case("REINHARDT_BASE_DIR", "/", true)]
	fn detects_env_overrides_of_the_publication_contract(
		#[case] key: &str,
		#[case] value: &str,
		#[case] conflicting: bool,
	) {
		// Arrange
		let env = BTreeMap::from([(key.to_owned(), value.to_owned())]);

		// Act
		let conflicts = conflicting_publication_env("/app/static", "/static/", &env);

		// Assert
		let expected: Vec<String> = if conflicting {
			vec![key.to_owned()]
		} else {
			Vec::new()
		};
		assert_eq!(conflicts, expected);
	}
}
