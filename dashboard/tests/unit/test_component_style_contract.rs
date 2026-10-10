//! Source-level contracts for Dashboard's generated component stylesheet.

use rstest::rstest;
use std::{
	fs,
	path::{Path, PathBuf},
};

const INDEX_HTML: &str = include_str!("../../index.html");
const AUTH_STYLE_SOURCE: &str = include_str!("../../src/apps/auth/client/style.rs");
const DASHBOARD_CLIENT_SOURCE: &str = include_str!("../../src/apps/dashboard/client.rs");
const DASHBOARD_LAYOUT_SOURCE: &str = include_str!("../../src/apps/dashboard/client/layout.rs");
const DASHBOARD_STYLE_SOURCE: &str = include_str!("../../src/apps/dashboard/client/style.rs");
const CLUSTERS_CLIENT_SOURCE: &str = include_str!("../../src/apps/clusters/client.rs");
const CLUSTERS_LIST_SOURCE: &str = include_str!("../../src/apps/clusters/client/pages/list.rs");
const CLUSTERS_STYLE_SOURCE: &str = include_str!("../../src/apps/clusters/client/style.rs");
const SHARED_IMPERATIVE_SOURCES: &[(&str, &str)] = &[
	(
		"entity_select",
		include_str!("../../src/shared/client/components/entity_select.rs"),
	),
	(
		"status_badge",
		include_str!("../../src/shared/client/components/status_badge.rs"),
	),
	(
		"toast",
		include_str!("../../src/shared/client/components/toast.rs"),
	),
	("websocket", include_str!("../../src/shared/client/ws.rs")),
];

const AUTH_PRODUCTION_SOURCES: &[(&str, &str)] = &[
	(
		"auth_layout",
		include_str!("../../src/apps/auth/client/components/auth_layout.rs"),
	),
	(
		"oauth_buttons",
		include_str!("../../src/apps/auth/client/components/oauth_buttons.rs"),
	),
	(
		"account",
		include_str!("../../src/apps/auth/client/pages/account.rs"),
	),
	(
		"login",
		include_str!("../../src/apps/auth/client/pages/login.rs"),
	),
	(
		"register",
		include_str!("../../src/apps/auth/client/pages/register.rs"),
	),
];

fn source_style_violations(source: &str) -> Vec<&'static str> {
	let mut violations = Vec::new();
	let lowercase_source = source.to_ascii_lowercase();

	for framework in ["unocss", "tailwind"] {
		if lowercase_source.contains(framework) {
			violations.push(match framework {
				"unocss" => "unocss reference",
				"tailwind" => "tailwind reference",
				_ => unreachable!("the framework list is fixed"),
			});
		}
	}

	if has_page_class_literal(source) {
		violations.push("raw page class literal");
	}
	if has_imperative_class_literal(source) {
		violations.push("imperative utility class literal");
	}
	if has_raw_html_class_literal(source) {
		violations.push("raw HTML utility class literal");
	}

	violations
}

fn has_page_class_literal(source: &str) -> bool {
	identifier_offsets(source, "class").any(|index| {
		let remainder = source[index + "class".len()..].trim_start();
		let Some(remainder) = remainder.strip_prefix(':') else {
			return false;
		};
		parse_rust_string_literal(remainder.trim_start()).is_some()
	})
}

fn has_imperative_class_literal(source: &str) -> bool {
	["set_attribute", "attr"].into_iter().any(|method| {
		identifier_offsets(source, method).any(|index| {
			let remainder = source[index + method.len()..].trim_start();
			let Some(remainder) = remainder.strip_prefix('(') else {
				return false;
			};
			let Some((attribute, consumed)) = parse_rust_string_literal(remainder.trim_start())
			else {
				return false;
			};
			if attribute != "class" {
				return false;
			}
			let remainder = remainder.trim_start()[consumed..].trim_start();
			let Some(remainder) = remainder.strip_prefix(',') else {
				return false;
			};
			parse_rust_string_literal(remainder.trim_start()).is_some()
		})
	})
}

fn has_raw_html_class_literal(source: &str) -> bool {
	let mut remainder = source;

	while let Some(character) = remainder.chars().next() {
		if let Some((literal, consumed)) = parse_rust_string_literal(remainder) {
			if has_html_class_literal(literal) {
				return true;
			}
			remainder = &remainder[consumed..];
		} else {
			remainder = &remainder[character.len_utf8()..];
		}
	}

	false
}

fn has_html_class_literal(source: &str) -> bool {
	let mut remainder = source;

	while let Some(tag_start) = remainder.find('<') {
		let tag = &remainder[tag_start + 1..];
		let Some(tag_end) = html_tag_end(tag) else {
			return false;
		};
		let tag = &tag[..tag_end];
		if has_html_class_attribute(tag) {
			return true;
		}
		remainder = &remainder[tag_start + tag_end + 2..];
	}

	false
}

fn has_html_class_attribute(tag: &str) -> bool {
	let mut quote = None;
	let mut index = 0;

	while index < tag.len() {
		let remainder = &tag[index..];
		if let Some((next_quote, width)) = html_quote(remainder) {
			if quote.is_none() {
				quote = Some(next_quote);
			} else if quote == Some(next_quote) {
				quote = None;
			}
			index += width;
			continue;
		}
		if quote.is_none()
			&& remainder
				.get(.."class".len())
				.is_some_and(|candidate| candidate.eq_ignore_ascii_case("class"))
			&& html_attribute_boundary(tag, index)
		{
			let remainder = remainder["class".len()..].trim_start();
			let Some(remainder) = remainder.strip_prefix('=') else {
				index += "class".len();
				continue;
			};
			let Some(value) = parse_html_attribute_value(remainder.trim_start()) else {
				return false;
			};
			let value = value.trim();
			return !(value.starts_with('{') && value.ends_with('}'));
		}
		index += remainder
			.chars()
			.next()
			.expect("tag is not empty")
			.len_utf8();
	}

	false
}

fn html_attribute_boundary(tag: &str, index: usize) -> bool {
	index == 0
		|| tag[..index]
			.chars()
			.next_back()
			.is_some_and(|character| character.is_ascii_whitespace())
}

fn html_tag_end(source: &str) -> Option<usize> {
	let mut quote = None;
	let mut index = 0;

	while index < source.len() {
		let remainder = &source[index..];
		if let Some((next_quote, width)) = html_quote(remainder) {
			if quote.is_none() {
				quote = Some(next_quote);
			} else if quote == Some(next_quote) {
				quote = None;
			}
			index += width;
			continue;
		}
		if quote.is_none() && remainder.starts_with('>') {
			return Some(index);
		}
		index += remainder.chars().next()?.len_utf8();
	}

	None
}

fn html_quote(source: &str) -> Option<(char, usize)> {
	if source.starts_with("\\\"") {
		return Some(('"', 2));
	}
	if source.starts_with("\\'") {
		return Some(('\'', 2));
	}
	let quote = source.chars().next()?;
	(quote == '"' || quote == '\'').then_some((quote, quote.len_utf8()))
}

fn identifier_offsets<'a>(
	source: &'a str,
	identifier: &'a str,
) -> impl Iterator<Item = usize> + 'a {
	source
		.match_indices(identifier)
		.filter_map(move |(index, _)| {
			let before = source[..index].chars().next_back();
			let after = source[index + identifier.len()..].chars().next();
			(!before.is_some_and(is_identifier_character)
				&& !after.is_some_and(is_identifier_character))
			.then_some(index)
		})
}

fn is_identifier_character(character: char) -> bool {
	character.is_ascii_alphanumeric() || character == '_'
}

fn parse_rust_string_literal(source: &str) -> Option<(&str, usize)> {
	if let Some(remainder) = source.strip_prefix('"') {
		let mut escaped = false;
		for (index, character) in remainder.char_indices() {
			if character == '"' && !escaped {
				return Some((&remainder[..index], index + 2));
			}
			escaped = character == '\\' && !escaped;
			if character != '\\' {
				escaped = false;
			}
		}
		return None;
	}

	let raw = source.strip_prefix('r')?;
	let hashes = raw
		.chars()
		.take_while(|character| *character == '#')
		.count();
	let remainder = raw.strip_prefix(&"#".repeat(hashes))?.strip_prefix('"')?;
	let terminator = format!("\"{}", "#".repeat(hashes));
	let index = remainder.find(&terminator)?;
	Some((
		&remainder[..index],
		1 + hashes + 1 + index + terminator.len(),
	))
}

fn parse_html_attribute_value(source: &str) -> Option<&str> {
	if let Some(remainder) = source.strip_prefix("\\\"") {
		let index = remainder.find("\\\"")?;
		return Some(&remainder[..index]);
	}

	let quote = source.chars().next()?;
	if quote == '"' || quote == '\'' {
		let remainder = &source[quote.len_utf8()..];
		let index = remainder.find(quote)?;
		return Some(&remainder[..index]);
	}

	let end = source
		.find(|character: char| character.is_whitespace() || character == '>')
		.unwrap_or(source.len());
	(end > 0).then_some(&source[..end])
}

fn production_client_sources() -> Vec<PathBuf> {
	let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
	let mut paths = Vec::new();
	collect_rust_sources(&source_root, &mut paths);

	paths
		.into_iter()
		.filter(|path| is_production_client_source(path, &source_root))
		.collect()
}

fn collect_rust_sources(directory: &Path, paths: &mut Vec<PathBuf>) {
	let mut entries = fs::read_dir(directory)
		.unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()))
		.map(|entry| entry.expect("source directory entry should be readable"))
		.collect::<Vec<_>>();
	entries.sort_by_key(|entry| entry.path());

	for entry in entries {
		let path = entry.path();
		if path.is_dir() {
			collect_rust_sources(&path, paths);
		} else if path.extension().is_some_and(|extension| extension == "rs") {
			paths.push(path);
		}
	}
}

fn is_production_client_source(path: &Path, source_root: &Path) -> bool {
	let components = path
		.strip_prefix(source_root)
		.expect("source path must remain under src")
		.components()
		.map(|component| component.as_os_str().to_string_lossy())
		.collect::<Vec<_>>();

	let app_client = components.len() >= 3
		&& components[0] == "apps"
		&& ((components.len() == 3 && components[2] == "client.rs")
			|| (components.len() >= 4 && components[2] == "client"));
	let shared_client =
		(components.len() == 2 && components[0] == "shared" && components[1] == "client.rs")
			|| (components.len() >= 3 && components[0] == "shared" && components[1] == "client");

	app_client || shared_client
}

#[rstest]
fn source_gate_rejects_literal_variants() {
	// Arrange
	let cases = [
		(
			"raw page string",
			"page!({ div { class:r#\"p-4\"# } });",
			vec!["raw page class literal"],
		),
		(
			"case-insensitive framework references",
			"let legacy = \"uNoCsS TaIlWiNd\";",
			vec!["unocss reference", "tailwind reference"],
		),
		(
			"newline page string",
			"page!({ div { class:\n\t\"p-4\" } });",
			vec!["raw page class literal"],
		),
		(
			"page string with whitespace before colon",
			"page!({ div { class : \"p-4\" } });",
			vec!["raw page class literal"],
		),
		(
			"imperative string",
			"element.set_attribute(\"class\",\"flex\");",
			vec!["imperative utility class literal"],
		),
		(
			"imperative raw string",
			"element.attr(\"class\", r###\"grid\"###);",
			vec!["imperative utility class literal"],
		),
		(
			"double quoted HTML",
			r##"element.set_inner_html(r#"<div class = "gap-4"></div>"#);"##,
			vec!["raw HTML utility class literal"],
		),
		(
			"single quoted HTML",
			r##"element.set_inner_html(r#"<div class='gap-4'></div>"#);"##,
			vec!["raw HTML utility class literal"],
		),
		(
			"escaped double quoted HTML",
			"element.set_inner_html(\"<div class=\\\"gap-4\\\"></div>\");",
			vec!["raw HTML utility class literal"],
		),
		(
			"unquoted HTML",
			r##"element.set_inner_html(r#"<div class=gap-4></div>"#);"##,
			vec!["raw HTML utility class literal"],
		),
		(
			"quoted greater-than HTML attribute",
			r##"element.set_inner_html(r#"<div data-label=">" class="gap-4"></div>"#);"##,
			vec!["raw HTML utility class literal"],
		),
		(
			"uppercase HTML class attribute",
			r##"element.set_inner_html(r#"<div CLASS="gap-4"></div>"#);"##,
			vec!["raw HTML utility class literal"],
		),
		(
			"mixed-case HTML class attribute",
			r##"element.set_inner_html(r#"<div Class="gap-4"></div>"#);"##,
			vec!["raw HTML utility class literal"],
		),
	];

	// Act + Assert
	for (name, source, expected) in cases {
		assert_eq!(source_style_violations(source), expected, "{name}");
	}
}

#[rstest]
fn source_gate_allows_generated_class_tokens() {
	// Arrange
	let source = r##"
		page!({ div { class: STYLES.card() + STYLES.selected() } });
		let class = STYLES.card();
		if width < breakpoint { let class = STYLES.compact(); }
		let classes: ClassList = STYLES.card() + selected;
		element.set_attribute("class", classes.as_str());
		element.set_inner_html(r#"<div class="{}"></div>"#);
		element.set_inner_html(r#"<div data-class="metadata"></div>"#);
		element.set_inner_html(r#"<div title="class=metadata"></div>"#);
	"##;

	// Act + Assert
	assert!(source_style_violations(source).is_empty());
}

#[rstest]
fn source_gate_limits_scanning_to_app_and_shared_client_modules() {
	// Arrange
	let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
	let cases = [
		("apps/auth/client.rs", true),
		("apps/auth/client/pages/login.rs", true),
		("shared/client.rs", true),
		("shared/client/components/toast.rs", true),
		("apps/github/services/client.rs", false),
		("apps/auth/services.rs", false),
		("client.rs", false),
	];

	// Act + Assert
	for (relative_path, expected) in cases {
		assert_eq!(
			is_production_client_source(&source_root.join(relative_path), &source_root),
			expected,
			"{relative_path}"
		);
	}
}

#[rstest]
fn production_client_sources_use_generated_style_tokens() {
	// Arrange
	let sources = production_client_sources();

	// Act
	let diagnostics = sources
		.iter()
		.flat_map(|path| {
			let source = fs::read_to_string(path)
				.unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
			source_style_violations(&source)
				.into_iter()
				.map(move |violation| format!("{}: {violation}", path.display()))
		})
		.collect::<Vec<_>>();

	// Assert
	assert!(
		!sources.is_empty(),
		"the Dashboard must contain client Rust modules"
	);
	assert!(
		diagnostics.is_empty(),
		"production client sources must use generated ClassToken/ClassList accessors:\n{}",
		diagnostics.join("\n")
	);
}

#[rstest]
fn generated_component_stylesheet_is_the_only_document_style_runtime() {
	// Arrange
	let document = INDEX_HTML.to_ascii_lowercase();

	// Act
	let forbidden_framework_references = ["unocss", "tailwind"]
		.into_iter()
		.map(|framework| (framework, document.matches(framework).count()))
		.collect::<Vec<_>>();
	let component_stylesheet_links = document.matches("__reinhardt__/components.css").count();
	let has_component_stylesheet_link = document.contains(
		r#"<link rel="stylesheet" href='{{ static_url("__reinhardt__/components.css") }}'>"#,
	);

	// Assert
	assert_eq!(
		forbidden_framework_references,
		[("unocss", 0), ("tailwind", 0)]
	);
	assert_eq!(component_stylesheet_links, 1);
	assert!(
		has_component_stylesheet_link,
		"the generated component stylesheet path must be a stylesheet link"
	);
}

#[rstest]
fn shared_imperative_dom_paths_do_not_embed_utility_class_literals() {
	// Arrange
	let utility_class_literals = [
		"\"bg-",
		"\"border-",
		"\"fixed ",
		"\"flex ",
		"\"gap-",
		"\"items-",
		"\"justify-",
		"\"max-w-",
		"\"min-w-",
		"\"p-",
		"\"px-",
		"\"py-",
		"\"rounded-",
		"\"shadow-",
		"\"text-",
		"\"top-",
		"\"right-",
		"\"z-",
	];

	// Act + Assert
	for (source_name, source) in SHARED_IMPERATIVE_SOURCES {
		for utility_class_literal in utility_class_literals {
			assert_eq!(
				source.matches(utility_class_literal).count(),
				0,
				"{source_name} must use typed shared style tokens"
			);
		}
	}
}

#[rstest]
fn auth_pages_and_components_use_typed_style_tokens() {
	// Arrange + Act + Assert
	for (source_name, source) in AUTH_PRODUCTION_SOURCES {
		assert!(
			!source.contains("class: \""),
			"{source_name} must use typed shared or auth-local style tokens"
		);
	}
}

#[rstest]
fn auth_account_grid_retains_the_desktop_two_column_rule() {
	// Arrange + Act
	let has_desktop_breakpoint = AUTH_STYLE_SOURCE.contains("@media (min-width: 1024px)");
	let has_two_columns = AUTH_STYLE_SOURCE
		.contains("grid-template-columns: unchecked_fn!(repeat(2, minmax(0, 1fr)));");

	// Assert
	assert!(
		has_desktop_breakpoint,
		"account layout must define a desktop breakpoint"
	);
	assert!(
		has_two_columns,
		"account layout must restore two desktop columns"
	);
}

#[rstest]
fn auth_actions_share_one_spacing_token() {
	// Arrange + Act
	let has_shared_spacing =
		AUTH_STYLE_SOURCE.contains(".account_action_spacing {\n\t\tmargin-top: 1.25rem;\n\t}");
	let has_duplicate_spacing = AUTH_STYLE_SOURCE.contains(".account_actions {")
		|| AUTH_STYLE_SOURCE.contains(".account_error_action {");

	// Assert
	assert!(
		has_shared_spacing,
		"auth actions must share one spacing token"
	);
	assert!(
		!has_duplicate_spacing,
		"auth actions must not define duplicate spacing tokens"
	);
}

#[rstest]
fn dashboard_shell_uses_typed_shared_and_dashboard_style_tokens() {
	// Arrange
	let dashboard_sources = [DASHBOARD_CLIENT_SOURCE, DASHBOARD_LAYOUT_SOURCE];

	// Act + Assert
	for source in dashboard_sources {
		assert!(
			!source.contains("class: \""),
			"dashboard client sources must use typed shared or dashboard-local style tokens"
		);
	}
	assert!(
		DASHBOARD_CLIENT_SOURCE.contains("pub mod style;"),
		"dashboard client module must export its local style module"
	);
}

#[rstest]
fn dashboard_overview_panels_preserve_the_desktop_asymmetric_ratio() {
	// Arrange + Act
	let has_desktop_ratio = DASHBOARD_STYLE_SOURCE
		.contains("@media (min-width: 1024px) {\n\t\t\tgrid-template-columns: (1.2fr, 0.8fr);");

	// Assert
	assert!(
		has_desktop_ratio,
		"desktop overview panels must retain the 1.2fr to 0.8fr ratio"
	);
}

#[rstest]
fn clusters_page_uses_typed_shared_and_cluster_style_tokens() {
	// Arrange
	let cluster_sources = [CLUSTERS_LIST_SOURCE];

	// Act + Assert
	for source in cluster_sources {
		assert!(
			!source.contains("class: \""),
			"cluster client sources must use typed shared or cluster-local style tokens"
		);
	}
	assert!(
		CLUSTERS_CLIENT_SOURCE.contains("pub mod style;"),
		"clusters client module must export its local style module"
	);
	assert!(
		CLUSTERS_STYLE_SOURCE.contains("pub static STYLES: ClustersStyles = style!"),
		"clusters must expose a unique generated style collection"
	);
	assert!(
		CLUSTERS_STYLE_SOURCE.contains("word-break: break-all;"),
		"one-time cluster tokens must break unbroken values before overflowing"
	);
	assert!(
		CLUSTERS_LIST_SOURCE.contains("class: STYLES.page_layout()"),
		"cluster page must render its generated responsive layout token"
	);
	assert!(
		CLUSTERS_LIST_SOURCE.contains("class: STYLES.token_value()"),
		"cluster token display must render its generated token token"
	);
	assert!(
		CLUSTERS_LIST_SOURCE.contains("STYLES.cluster_badge() + self::cluster_badge_state"),
		"cluster inventory badges must compose generated base and state tokens"
	);
	assert!(
		CLUSTERS_LIST_SOURCE.contains("class: SHARED_STYLES.shell(),\n\t\t\tdiv {\n\t\t\t\tdiv {"),
		"the page shell must not add a second outer content stack"
	);
	assert!(
		CLUSTERS_LIST_SOURCE.contains("class: SHARED_STYLES.muted() + STYLES.intro()"),
		"the cluster introduction must retain its local top margin"
	);
}
