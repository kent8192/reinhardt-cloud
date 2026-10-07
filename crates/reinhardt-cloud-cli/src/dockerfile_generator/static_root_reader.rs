//! Select the non-secret static output path from the production TOML profile.
use std::path::{Component, Path};

use reinhardt_cloud_types::reinhardt_cloud_toml::ReinhardtCloudToml;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StaticRoot {
	path: String,
	pub(super) env_binding: Option<(String, String)>,
}

impl StaticRoot {
	#[cfg(test)]
	pub(super) fn relative(path: &str) -> Self {
		Self {
			path: path.to_owned(),
			env_binding: None,
		}
	}

	pub(super) fn build_path(&self, member: Option<&str>) -> String {
		if self.path.starts_with('/') {
			self.path.clone()
		} else {
			member.map_or_else(
				|| format!("/app/{}", self.path),
				|member| format!("/app/{member}/{}", self.path),
			)
		}
	}

	pub(super) fn runtime_path(&self) -> String {
		if self.path.starts_with('/') {
			self.path.clone()
		} else {
			format!("/app/{}", self.path)
		}
	}
}

pub(super) fn read_static_root(
	project: &Path,
	config: &ReinhardtCloudToml,
) -> Result<StaticRoot, String> {
	let mut selected: [Option<toml::Value>; 3] = [None, None, None];
	for profile in ["base", "production"] {
		let path = project.join("settings").join(format!("{profile}.toml"));
		if !path.exists() {
			continue;
		}
		let text = std::fs::read_to_string(&path)
			.map_err(|error| format!("cannot read static settings {}: {error}", path.display()))?;
		let value: toml::Value = toml::from_str(&text)
			.map_err(|error| format!("cannot parse static settings {}: {error}", path.display()))?;
		// A relocated/dynamic base directory requires a custom Dockerfile; silently
		// assuming the member working directory would copy the wrong publication.
		for base in [
			value.get("core").and_then(|core| core.get("base_dir")),
			value.get("base_dir"),
		]
		.into_iter()
		.flatten()
		{
			if base.as_str() != Some(".") {
				return Err("Pages Dockerfile generation cannot resolve a custom base_dir; provide a custom Dockerfile".to_owned());
			}
		}
		for (index, candidate) in [
			value.get("static_files").and_then(|v| v.get("root")),
			value.get("static").and_then(|v| v.get("root")),
			value.get("static_root"),
		]
		.into_iter()
		.enumerate()
		{
			if let Some(candidate) = candidate {
				selected[index] = Some(candidate.clone());
			}
		}
	}
	let raw = selected.into_iter().flatten().next().ok_or("Pages Dockerfile generation requires an explicit static root in settings/base.toml or settings/production.toml; use a custom Dockerfile for dynamic settings")?;
	let raw = raw.as_str().ok_or("static root must be a string")?;
	let (path, env_binding) = if let Some(expression) =
		raw.strip_prefix("${").and_then(|v| v.strip_suffix('}'))
	{
		let (name, fallback) = expression
			.split_once(":-")
			.map_or((expression, None), |(name, fallback)| {
				(name, Some(fallback))
			});
		if !name.ends_with("_ROOT")
			|| !name
				.chars()
				.all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
		{
			return Err("static root interpolation must reference a path variable ending in _ROOT; otherwise provide a custom Dockerfile".to_owned());
		}
		let value = config
			.source
			.as_ref()
			.and_then(|source| source.build.as_ref())
			.and_then(|build| build.build_args.get(name))
			.map(String::as_str)
			.or(fallback)
			.ok_or(
				"static root variable requires a source.build.build_args value or a literal default",
			)?
			.to_owned();
		(value.clone(), Some((name.to_owned(), value)))
	} else {
		(raw.to_owned(), None)
	};
	let relative = path.strip_prefix("/app/").unwrap_or(&path);
	if path.starts_with('/') && !path.starts_with("/app/") {
		return Err("generated Pages images require an absolute static root under /app; provide a custom Dockerfile for other locations".to_owned());
	}
	let components: Vec<_> = Path::new(relative).components().collect();
	if components.is_empty()
		|| components
			.iter()
			.any(|component| !matches!(component, Component::Normal(_)))
		|| !relative
			.chars()
			.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '-' | '_'))
	{
		return Err("static root must be a nonempty Dockerfile-safe directory without traversal or interpolation".to_owned());
	}
	if [
		"settings",
		"src",
		"target",
		"migrations",
		".git",
		".agents",
		".codex",
	]
	.iter()
	.any(|reserved| relative.split('/').next() == Some(*reserved))
	{
		return Err("static root overlaps application sources, settings, or build metadata; use a dedicated publication directory".to_owned());
	}
	Ok(StaticRoot { path, env_binding })
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	#[rstest]
	#[case(
		"[static_files]\nroot='dist'",
		"[static_files]\nroot='public/assets'",
		"public/assets"
	)]
	#[case("[static]\nroot='assets'", "", "assets")]
	#[case("static_root='/app/published'", "", "/app/published")]
	#[case(
		"[static_files]\nroot='typed'\n[static]\nroot='modern'",
		"[static]\nroot='production'",
		"typed"
	)]
	fn resolves_composed_root(
		#[case] base: &str,
		#[case] production: &str,
		#[case] expected: &str,
	) {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join("settings")).unwrap();
		std::fs::write(dir.path().join("settings/base.toml"), base).unwrap();
		std::fs::write(dir.path().join("settings/production.toml"), production).unwrap();
		let root = read_static_root(dir.path(), &ReinhardtCloudToml::default()).unwrap();
		assert_eq!(
			root.runtime_path(),
			if expected.starts_with('/') {
				expected.to_owned()
			} else {
				format!("/app/{expected}")
			}
		);
		assert_eq!(
			root.build_path(Some("dashboard")),
			if expected.starts_with('/') {
				expected.to_owned()
			} else {
				format!("/app/dashboard/{expected}")
			}
		);
	}
	#[rstest]
	#[case("")]
	#[case("..")]
	#[case("../secrets")]
	#[case("/app")]
	#[case("/etc")]
	#[case("settings")]
	#[case("src/assets")]
	#[case("$(cat secret)")]
	#[case("assets\nRUN malicious")]
	fn refuses_unsafe_root(#[case] path: &str) {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join("settings")).unwrap();
		std::fs::write(
			dir.path().join("settings/base.toml"),
			format!("static_root={path:?}"),
		)
		.unwrap();
		assert!(read_static_root(dir.path(), &ReinhardtCloudToml::default()).is_err());
	}
	#[rstest]
	fn pins_static_path_interpolation_without_loading_secrets() {
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join("settings")).unwrap();
		std::fs::write(dir.path().join("settings/base.toml"), "[core]\nsecret_key='${SECRET_KEY}'\n[static_files]\nroot='${REINHARDT_STATIC_FILES__ROOT:-dist}'").unwrap();
		let root = read_static_root(dir.path(), &ReinhardtCloudToml::default()).unwrap();
		assert_eq!(root.runtime_path(), "/app/dist");
		assert_eq!(
			root.env_binding,
			Some(("REINHARDT_STATIC_FILES__ROOT".to_owned(), "dist".to_owned()))
		);
	}
}
