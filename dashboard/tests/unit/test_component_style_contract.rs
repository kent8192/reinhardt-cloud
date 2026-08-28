//! Source-level contracts for Dashboard's generated component stylesheet.

const INDEX_HTML: &str = include_str!("../../index.html");
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

#[test]
fn generated_component_stylesheet_is_the_only_document_style_runtime() {
	// Arrange
	let document = INDEX_HTML.to_ascii_lowercase();

	// Act
	let unocss_references = document.matches("unocss").count();
	let component_stylesheet_links = document.matches("__reinhardt__/components.css").count();

	// Assert
	assert_eq!(unocss_references, 0);
	assert_eq!(component_stylesheet_links, 1);
}

#[test]
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
