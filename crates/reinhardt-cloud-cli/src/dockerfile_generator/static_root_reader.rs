//! Select the non-secret static output path from the production TOML profile.
use std::collections::BTreeSet;
use std::path::{Component, Path};

use reinhardt_cloud_types::reinhardt_cloud_toml::ReinhardtCloudToml;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StaticRoot {
	path: String,
	pub(super) url: String,
	pub(super) url_env_binding: Option<(String, String)>,
	pub(super) env_binding: Option<(String, String)>,
	pub(super) build_env: Vec<String>,
}

impl StaticRoot {
	#[cfg(test)]
	pub(super) fn relative(path: &str) -> Self {
		Self {
			path: path.to_owned(),
			url: "/static/".to_owned(),
			url_env_binding: None,
			env_binding: None,
			build_env: Vec::new(),
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
	// Compose the production profile the way the settings loader does: later
	// profiles override earlier ones key by key. buildstatic interpolates only
	// effective values, so variables in shadowed values must not be injected.
	let mut value = toml::Value::Table(toml::Table::new());
	for profile in ["base", "production"] {
		let path = project.join("settings").join(format!("{profile}.toml"));
		if !path.exists() {
			continue;
		}
		let text = std::fs::read_to_string(&path)
			.map_err(|error| format!("cannot read static settings {}: {error}", path.display()))?;
		let overlay: toml::Value = toml::from_str(&text)
			.map_err(|error| format!("cannot parse static settings {}: {error}", path.display()))?;
		merge_profile(&mut value, overlay);
	}
	let mut build_env = BTreeSet::new();
	collect_required_variables(&value, &mut build_env)?;
	let base_dirs = [
		value.get("core").and_then(|core| core.get("base_dir")),
		value.get("base_dir"),
	];
	let selected = [
		value.get("static_files").and_then(|v| v.get("root")),
		value.get("static").and_then(|v| v.get("root")),
		value.get("static_root"),
	];
	let selected_urls = [
		value.get("static_files").and_then(|v| v.get("url")),
		value.get("static").and_then(|v| v.get("url")),
		value.get("static_url"),
	];
	let url = selected_urls.into_iter().flatten().next();
	let url = match &url {
		Some(value) => value.as_str().ok_or("static URL must be a string")?,
		None => "/static/",
	};
	let (url, url_env_binding) = if let Some(expression) =
		url.strip_prefix("${").and_then(|v| v.strip_suffix('}'))
	{
		let (name, fallback) = expression
			.split_once(":-")
			.map_or((expression, None), |(name, value)| (name, Some(value)));
		insert_build_variable(name, &mut BTreeSet::new())?;
		if !name.ends_with("_URL") {
			return Err("static URL interpolation must reference a variable ending in _URL; provide a custom Dockerfile for other settings".to_owned());
		}
		let value = config
			.source
			.as_ref()
			.and_then(|source| source.build.as_ref())
			.and_then(|build| build.build_args.get(name))
			.map(String::as_str)
			.or(fallback)
			.ok_or(
				"static URL variable requires a source.build.build_args value or a literal default",
			)?
			.to_owned();
		(value.clone(), Some((name.to_owned(), value)))
	} else {
		(url.to_owned(), None)
	};
	if !url.starts_with('/')
		|| !url.ends_with('/')
		|| !url
			.chars()
			.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '/' | '.' | '-' | '_'))
		|| Path::new(&url)
			.components()
			.any(|part| matches!(part, Component::ParentDir))
	{
		return Err("generated Pages require a Dockerfile-safe static URL starting and ending with '/'; provide a custom Dockerfile for other settings".to_owned());
	}
	// Ingress keeps an existing application route for a duplicate path, so a
	// root prefix would never reach the static-server sidecar.
	if !Path::new(&url)
		.components()
		.any(|part| matches!(part, Component::Normal(_)))
	{
		return Err("generated Pages require a static URL below '/' (for example '/static/') so it cannot collide with application routes; provide a custom Dockerfile for other settings".to_owned());
	}
	// A leading `//` makes browsers treat generated references as
	// protocol-relative URLs to another host, and empty or `.` segments do not
	// match the normalized Ingress path; require single-slash-separated segments.
	if url[1..url.len() - 1]
		.split('/')
		.any(|segment| segment.is_empty() || segment == ".")
	{
		return Err("generated Pages require a normalized static URL path such as '/assets/' without empty or '.' segments (a leading '//' is a protocol-relative URL to another host); provide a custom Dockerfile for other settings".to_owned());
	}
	// Validate the effective value after production has overridden base settings.
	if let Some(base) = base_dirs.into_iter().flatten().next()
		&& base.as_str() != Some(".")
	{
		return Err("Pages Dockerfile generation cannot resolve a custom base_dir; provide a custom Dockerfile".to_owned());
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
	let relative_path = Path::new(relative);
	let member = super::compute_project_relative_path(project);
	let publication = if path.starts_with("/app/") {
		member
			.as_deref()
			.and_then(|member| relative_path.strip_prefix(member).ok())
			.unwrap_or(relative_path)
	} else {
		relative_path
	};
	if publication.as_os_str().is_empty()
		|| [
			"settings",
			"src",
			"target",
			"migrations",
			".git",
			".agents",
			".codex",
		]
		.iter()
		.any(|reserved| publication.starts_with(reserved))
	{
		return Err("static root overlaps application sources, settings, or build metadata; use a dedicated publication directory".to_owned());
	}
	// The asset stage writes, and the runtime image copies, the whole directory
	// from the workspace tree in the builder. It must not be an ancestor of the
	// application member (e.g. `/app/apps` for member `apps/dashboard`) or hold
	// any crate sources of its own.
	let build_relative = if path.starts_with("/app/") {
		relative_path.to_path_buf()
	} else {
		Path::new(member.as_deref().unwrap_or_default()).join(relative_path)
	};
	let workspace = super::locate_workspace_file(project, "Cargo.lock")
		.and_then(|lock| lock.parent().map(Path::to_path_buf))
		.unwrap_or_else(|| project.to_path_buf());
	if Path::new(member.as_deref().unwrap_or_default()).starts_with(&build_relative)
		|| contains_crate_manifest(&workspace.join(&build_relative))?
	{
		return Err("static root contains a workspace member or crate sources; use a dedicated publication directory".to_owned());
	}
	if let Some((name, _)) = &env_binding {
		build_env.remove(name);
	}
	if let Some((name, _)) = &url_env_binding {
		build_env.remove(name);
	}
	Ok(StaticRoot {
		path,
		url,
		url_env_binding,
		env_binding,
		build_env: build_env.into_iter().collect(),
	})
}

/// Whether an existing directory holds a `Cargo.toml` at any depth, without
/// following symbolic links. A missing directory holds no sources.
fn contains_crate_manifest(directory: &Path) -> Result<bool, String> {
	let mut pending = vec![directory.to_path_buf()];
	while let Some(current) = pending.pop() {
		let entries = match std::fs::read_dir(&current) {
			Ok(entries) => entries,
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
			Err(error) if error.kind() == std::io::ErrorKind::NotADirectory => continue,
			Err(error) => {
				return Err(format!(
					"cannot inspect static root {}: {error}",
					current.display()
				));
			}
		};
		for entry in entries {
			let entry = entry.map_err(|error| {
				format!("cannot inspect static root {}: {error}", current.display())
			})?;
			let kind = entry.file_type().map_err(|error| {
				format!(
					"cannot inspect static root {}: {error}",
					entry.path().display()
				)
			})?;
			if kind.is_dir() {
				pending.push(entry.path());
			} else if entry.file_name() == "Cargo.toml" {
				return Ok(true);
			}
		}
	}
	Ok(false)
}

/// Overlay a later settings profile: tables merge recursively, other values replace.
fn merge_profile(target: &mut toml::Value, overlay: toml::Value) {
	match (target, overlay) {
		(toml::Value::Table(target), toml::Value::Table(overlay)) => {
			for (key, value) in overlay {
				match target.get_mut(&key) {
					Some(existing) => merge_profile(existing, value),
					None => {
						target.insert(key, value);
					}
				}
			}
		}
		(target, overlay) => *target = overlay,
	}
}

/// Read variable names only. Asset publication never needs deployment credentials.
/// Temporary random values satisfy eager TOML interpolation for unrelated settings.
fn collect_required_variables(
	value: &toml::Value,
	names: &mut BTreeSet<String>,
) -> Result<(), String> {
	match value {
		toml::Value::String(text) => {
			let mut remaining = text.as_str();
			while let Some((_, expression)) = remaining.split_once("${") {
				let Some((expression, suffix)) = expression.split_once('}') else {
					break;
				};
				remaining = suffix;
				let name = match expression.split_once(':') {
					None => expression,
					Some((_, modifier)) if modifier.starts_with('-') => continue,
					Some((name, modifier)) if modifier.starts_with('?') => name,
					Some(_) => {
						return Err("unsupported required setting interpolation; provide a custom Dockerfile".to_owned());
					}
				};
				insert_build_variable(name, names)?;
			}
		}
		toml::Value::Table(table) => {
			if let Some(toml::Value::String(name)) = table.get("env") {
				insert_build_variable(name, names)?;
			}
			for (key, value) in table {
				if key != "env" {
					collect_required_variables(value, names)?;
				}
			}
		}
		toml::Value::Array(values) => {
			for value in values {
				collect_required_variables(value, names)?;
			}
		}
		_ => {}
	}
	Ok(())
}

fn insert_build_variable(name: &str, names: &mut BTreeSet<String>) -> Result<(), String> {
	if !name.starts_with(|ch: char| ch.is_ascii_uppercase() || ch == '_')
		|| !name
			.chars()
			.all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '_')
		|| matches!(
			name,
			"PATH" | "HOME" | "SHELL" | "ENV" | "BASH_ENV" | "REINHARDT_ENV"
		) || name.starts_with("LD_")
		|| name.starts_with("DYLD_")
		|| name.starts_with("CARGO_")
		|| name.starts_with("RUST")
	{
		return Err("required setting uses an unsupported build environment variable; provide a custom Dockerfile".to_owned());
	}
	names.insert(name.to_owned());
	Ok(())
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

	#[rstest]
	#[case("${REINHARDT_STATIC_FILES__URL:-/assets/}", None)]
	#[case("${REINHARDT_STATIC_FILES__URL}", Some("/assets/"))]
	fn pins_static_url_without_reading_the_environment(
		#[case] value: &str,
		#[case] argument: Option<&str>,
	) {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join("settings")).unwrap();
		std::fs::write(
			dir.path().join("settings/base.toml"),
			format!("[static_files]\nroot='dist'\nurl='{value}'"),
		)
		.unwrap();
		let mut config = ReinhardtCloudToml::default();
		if let Some(argument) = argument {
			config.source = Some(reinhardt_cloud_types::reinhardt_cloud_toml::SourceSection {
				build: Some(reinhardt_cloud_types::reinhardt_cloud_toml::BuildSection {
					build_args: std::collections::BTreeMap::from([(
						"REINHARDT_STATIC_FILES__URL".to_owned(),
						argument.to_owned(),
					)]),
					..Default::default()
				}),
				..Default::default()
			});
		}

		// Act
		let root = read_static_root(dir.path(), &config).unwrap();

		// Assert
		assert_eq!(root.url, "/assets/");
		assert_eq!(
			root.url_env_binding,
			Some((
				"REINHARDT_STATIC_FILES__URL".to_owned(),
				"/assets/".to_owned()
			))
		);
		assert!(
			!root
				.build_env
				.iter()
				.any(|name| name == "REINHARDT_STATIC_FILES__URL")
		);
	}

	#[rstest]
	#[case("assets/")]
	#[case("/assets")]
	#[case("/assets/../private/")]
	#[case("/assets;echo injected/")]
	fn rejects_unsafe_static_urls(#[case] value: &str) {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join("settings")).unwrap();
		std::fs::write(
			dir.path().join("settings/base.toml"),
			format!("[static_files]\nroot='dist'\nurl='{value}'"),
		)
		.unwrap();

		// Act
		let result = read_static_root(dir.path(), &ReinhardtCloudToml::default());

		// Assert
		assert!(result.unwrap_err().contains("custom Dockerfile"));
	}

	#[rstest]
	#[case("/")]
	#[case("//")]
	#[case("/./")]
	fn rejects_static_urls_that_collide_with_application_routes(#[case] value: &str) {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join("settings")).unwrap();
		std::fs::write(
			dir.path().join("settings/base.toml"),
			format!("[static_files]\nroot='dist'\nurl='{value}'"),
		)
		.unwrap();

		// Act
		let result = read_static_root(dir.path(), &ReinhardtCloudToml::default());

		// Assert
		assert!(result.unwrap_err().contains("below '/'"));
	}

	#[rstest]
	#[case("//assets/", false)]
	#[case("///assets/", false)]
	#[case("/assets//", false)]
	#[case("/assets//js/", false)]
	#[case("/./assets/", false)]
	#[case("/assets/", true)]
	#[case("/assets/js/", true)]
	fn requires_single_slash_separated_static_urls(#[case] value: &str, #[case] accepted: bool) {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join("settings")).unwrap();
		std::fs::write(
			dir.path().join("settings/base.toml"),
			format!("[static_files]\nroot='dist'\nurl='{value}'"),
		)
		.unwrap();

		// Act
		let result = read_static_root(dir.path(), &ReinhardtCloudToml::default());

		// Assert
		match result {
			Ok(root) => {
				assert!(accepted, "{value} must be rejected");
				assert_eq!(root.url, value);
			}
			Err(error) => {
				assert!(!accepted, "{value} must be accepted: {error}");
				assert!(error.contains("protocol-relative"), "{error}");
			}
		}
	}

	#[rstest]
	#[case("[core]\nbase_dir='/old'", "[core]\nbase_dir='.'", true)]
	#[case("base_dir='/old'", "base_dir='.'", true)]
	#[case("[core]\nbase_dir='.'", "[core]\nbase_dir='/new'", false)]
	#[case("[core]\nbase_dir='/old'", "", false)]
	fn validates_effective_base_dir(
		#[case] base: &str,
		#[case] production: &str,
		#[case] accepted: bool,
	) {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join("settings")).unwrap();
		std::fs::write(
			dir.path().join("settings/base.toml"),
			format!("static_root='dist'\n{base}"),
		)
		.unwrap();
		std::fs::write(dir.path().join("settings/production.toml"), production).unwrap();
		// Act / Assert
		assert_eq!(
			read_static_root(dir.path(), &ReinhardtCloudToml::default()).is_ok(),
			accepted
		);
	}

	#[rstest]
	fn extracts_only_required_variable_names() {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join("settings")).unwrap();
		std::fs::write(dir.path().join("settings/base.toml"), "secret='${SECRET:?required}'\nkey='${API_KEY}'\noptional='${OPTIONAL:-default}'\npassword={env='DB_PASSWORD'}\nstatic_root='${ASSET_ROOT:-dist}'").unwrap();
		// Act
		let root = read_static_root(dir.path(), &ReinhardtCloudToml::default()).unwrap();
		// Assert
		assert_eq!(root.build_env, ["API_KEY", "DB_PASSWORD", "SECRET"]);
		assert_eq!(
			root.env_binding,
			Some(("ASSET_ROOT".to_owned(), "dist".to_owned()))
		);
	}

	#[rstest]
	#[case(
		"[static_files]\nroot='${REINHARDT_STATIC_FILES__ROOT}'",
		"[static_files]\nroot='dist'",
		vec![]
	)]
	#[case(
		"[core]\nsecret_key='${OLD_SECRET}'\n[static_files]\nroot='dist'",
		"[core]\nsecret_key='${NEW_SECRET}'",
		vec!["NEW_SECRET"]
	)]
	#[case(
		"[core]\nsecret_key='${SECRET}'\n[static_files]\nroot='dist'",
		"[core]\ndebug=false",
		vec!["SECRET"]
	)]
	fn injects_only_variables_of_the_effective_production_profile(
		#[case] base: &str,
		#[case] production: &str,
		#[case] required: Vec<&str>,
	) {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join("settings")).unwrap();
		std::fs::write(dir.path().join("settings/base.toml"), base).unwrap();
		std::fs::write(dir.path().join("settings/production.toml"), production).unwrap();

		// Act
		let root = read_static_root(dir.path(), &ReinhardtCloudToml::default()).unwrap();

		// Assert
		assert_eq!(root.build_env, required);
		assert_eq!(root.runtime_path(), "/app/dist");
		assert_eq!(root.env_binding, None);
	}

	#[rstest]
	#[case("${SECRET}", true, vec!["SECRET"])]
	#[case("${SECRET:?use a value:-hint}", true, vec!["SECRET"])]
	#[case("${SECRET:-default}", true, vec![])]
	#[case("${SECRET:+other}", false, vec![])]
	#[case("${SECRET:invalid}", false, vec![])]
	fn validates_required_interpolation_modifiers(
		#[case] expression: &str,
		#[case] accepted: bool,
		#[case] required: Vec<&str>,
	) {
		// Arrange
		let dir = tempfile::tempdir().unwrap();
		std::fs::create_dir(dir.path().join("settings")).unwrap();
		std::fs::write(
			dir.path().join("settings/base.toml"),
			format!("static_root='dist'\nsecret={expression:?}"),
		)
		.unwrap();

		// Act
		let result = read_static_root(dir.path(), &ReinhardtCloudToml::default());

		// Assert
		assert_eq!(result.is_ok(), accepted);
		if let Ok(root) = result {
			assert_eq!(root.build_env, required);
		}
	}

	#[rstest]
	#[case("EVIL;echo")]
	#[case("PATH")]
	#[case("REINHARDT_ENV")]
	#[case("LD_PRELOAD")]
	fn rejects_required_build_control_variables(#[case] name: &str) {
		// Arrange
		let mut names = BTreeSet::new();
		// Act / Assert
		assert!(insert_build_variable(name, &mut names).is_err());
		assert!(names.is_empty());
	}

	#[rstest]
	#[case("/app/dashboard", false)]
	#[case("/app/dashboard/settings", false)]
	#[case("/app/dashboard//settings", false)]
	#[case("/app/dashboard/./settings", false)]
	#[case("/app/dashboard/static", true)]
	#[case("/app/published", true)]
	fn validates_absolute_root_against_the_workspace_member(
		#[case] path: &str,
		#[case] accepted: bool,
	) {
		// Arrange: dashboard is a member under the Docker context root.
		let workspace = tempfile::tempdir().unwrap();
		std::fs::write(
			workspace.path().join("Cargo.toml"),
			"[workspace]\nmembers=['dashboard']",
		)
		.unwrap();
		std::fs::write(workspace.path().join("Cargo.lock"), "version = 3").unwrap();
		let project = workspace.path().join("dashboard");
		std::fs::create_dir_all(project.join("settings")).unwrap();
		std::fs::write(
			project.join("settings/base.toml"),
			format!("static_root={path:?}"),
		)
		.unwrap();

		// Act / Assert: reject paths that would copy sources or settings.
		assert_eq!(
			read_static_root(&project, &ReinhardtCloudToml::default()).is_ok(),
			accepted
		);
	}

	#[rstest]
	#[case("/app/apps", false)]
	#[case("/app/apps/dashboard", false)]
	#[case("/app/crates", false)]
	#[case("vendor", false)]
	#[case("/app/apps/dashboard/static", true)]
	#[case("static", true)]
	#[case("/app/published", true)]
	fn rejects_roots_containing_workspace_sources(#[case] path: &str, #[case] accepted: bool) {
		// Arrange: a nested member next to another crate and vendored sources.
		let workspace = tempfile::tempdir().unwrap();
		std::fs::write(
			workspace.path().join("Cargo.toml"),
			"[workspace]\nmembers=['apps/dashboard', 'crates/shared']",
		)
		.unwrap();
		std::fs::write(workspace.path().join("Cargo.lock"), "version = 3").unwrap();
		let project = workspace.path().join("apps/dashboard");
		std::fs::create_dir_all(project.join("settings")).unwrap();
		std::fs::write(project.join("Cargo.toml"), "[package]\nname='dashboard'").unwrap();
		std::fs::create_dir_all(workspace.path().join("crates/shared")).unwrap();
		std::fs::write(
			workspace.path().join("crates/shared/Cargo.toml"),
			"[package]\nname='shared'",
		)
		.unwrap();
		std::fs::create_dir_all(project.join("vendor/dependency")).unwrap();
		std::fs::write(
			project.join("vendor/dependency/Cargo.toml"),
			"[package]\nname='dependency'",
		)
		.unwrap();
		std::fs::write(
			project.join("settings/base.toml"),
			format!("static_root={path:?}"),
		)
		.unwrap();

		// Act
		let result = read_static_root(&project, &ReinhardtCloudToml::default());

		// Assert
		assert_eq!(result.is_ok(), accepted, "{result:?}");
		if !accepted {
			assert!(result.unwrap_err().contains("static root"));
		}
	}
}
