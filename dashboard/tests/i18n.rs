//! Tests for the i18n catalog and the page i18n context.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use cloud_control_plane::i18n::{DEFAULT_LOCALE, en, english_catalog, i18n_context};
use reinhardt::pages::i18n::{
	I18nContext, MessageCatalog, TranslationContext, provide_i18n_context,
};
use reinhardt::pages::reactive::ReactiveScope;
use reinhardt::pages::testing::component::render;
use reinhardt::pages::{page, t};
use rstest::rstest;
use serial_test::serial;

/// Collects every `.rs` file under `directory`.
fn rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
	let entries = fs::read_dir(directory).expect("source directory should be readable");
	for entry in entries {
		let path = entry.expect("directory entry should be readable").path();
		if path.is_dir() {
			rust_files(&path, files);
		} else if path.extension().is_some_and(|extension| extension == "rs") {
			files.push(path);
		}
	}
}

/// Returns the string-literal message IDs passed to `t!` in `source`.
///
/// The scanner recognises only standalone `t!("literal")` calls. Messages built
/// through `tr`, `tn`, `tp`, `tnp`, or a non-literal argument are not seen, so
/// keep those IDs in the catalog by hand.
fn t_messages(source: &str) -> Vec<String> {
	let bytes = source.as_bytes();
	let mut messages = Vec::new();
	let mut search_from = 0;
	while let Some(offset) = source[search_from..].find("t!(") {
		let start = search_from + offset;
		search_from = start + "t!(".len();
		let standalone =
			start == 0 || !(bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_');
		let rest = source[search_from..].trim_start();
		if !standalone || !rest.starts_with('"') {
			continue;
		}
		let mut message = String::new();
		let mut characters = rest[1..].chars();
		while let Some(character) = characters.next() {
			match character {
				'\\' => {
					if let Some(escaped) = characters.next() {
						message.push(match escaped {
							'n' => '\n',
							't' => '\t',
							other => other,
						});
					}
				}
				'"' => break,
				other => message.push(other),
			}
		}
		messages.push(message);
	}
	messages
}

#[rstest]
fn the_message_scanner_reads_only_standalone_t_literals() {
	// Arrange
	let source = r#"
		t!("Plain");
		t!( "Spaced" );
		t!("Escaped \"quote\"");
		format!("not a message");
		assert!(true);
		t!(dynamic_expression);
	"#;

	// Act
	let messages = t_messages(source);

	// Assert
	assert_eq!(messages, vec!["Plain", "Spaced", "Escaped \"quote\""]);
}

#[rstest]
fn every_t_message_has_an_english_entry() {
	// Arrange
	let mut files = Vec::new();
	rust_files(
		&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
		&mut files,
	);
	let catalog = english_catalog();

	// Act
	let missing: Vec<String> = files
		.iter()
		.flat_map(|file| {
			let source = fs::read_to_string(file).expect("source file should be readable");
			t_messages(&source)
		})
		.filter(|message| catalog.get(message).is_none())
		.collect();

	// Assert
	assert_eq!(missing, Vec::<String>::new());
}

#[rstest]
fn message_ids_are_unique_and_the_english_rendering_is_not_empty() {
	// Arrange
	let mut seen = HashSet::new();

	// Act
	let duplicates: Vec<&str> = en::MESSAGES
		.iter()
		.filter(|(message, _)| !seen.insert(*message))
		.map(|(message, _)| *message)
		.collect();
	let empty: Vec<&str> = en::MESSAGES
		.iter()
		.filter(|(_, translation)| translation.is_empty())
		.map(|(message, _)| *message)
		.collect();

	// Assert
	assert_eq!(duplicates, Vec::<&str>::new());
	assert_eq!(empty, Vec::<&str>::new());
}

#[rstest]
fn english_catalog_serves_the_default_locale() {
	// Arrange
	let context = i18n_context();

	// Act
	let locale = context.locale();

	// Assert
	assert_eq!(locale, DEFAULT_LOCALE);
	assert_eq!(
		context.translate("Skip to main content"),
		"Skip to main content"
	);
}

#[rstest]
#[serial(i18n)]
#[tokio::test]
async fn t_renders_the_active_catalog_and_follows_locale_changes() {
	// Arrange
	let mut translations = TranslationContext::new(DEFAULT_LOCALE, DEFAULT_LOCALE);
	translations
		.add_catalog(DEFAULT_LOCALE, english_catalog())
		.expect("the default locale tag is valid");
	// Test-only locale proving that catalogs other than English plug into
	// the same context; its message is not part of the shipped catalog.
	let mut test_locale = MessageCatalog::new("xx");
	test_locale.add_translation("Copy", "Kopieren");
	translations
		.add_catalog("xx", test_locale)
		.expect("the test locale tag is valid");
	let context = I18nContext::new(translations);
	let _i18n = provide_i18n_context(context.clone());
	let screen = render(|| {
		page!({
			p { { t!("Copy") } }
		})
	});
	let english = screen.get_by_text("Copy").text();

	// Act
	context
		.set_locale("xx")
		.expect("the test locale tag is valid");
	screen.settle().await;
	let translated = screen.get_by_text("Kopieren").text();

	// Assert
	assert_eq!(english, "Copy");
	assert_eq!(translated, "Kopieren");
}

#[rstest]
#[serial(i18n)]
fn unknown_locale_falls_back_to_the_english_source_text() {
	ReactiveScope::run(|| {
		// Arrange
		let context = i18n_context();
		let _i18n = provide_i18n_context(context.clone());
		context
			.set_locale("fr")
			.expect("the locale tag is well-formed");

		// Act
		let screen = render(|| {
			page!({
				p { { t!("Skip to main content") } }
			})
		});

		// Assert
		assert_eq!(
			screen.get_by_text("Skip to main content").text(),
			"Skip to main content"
		);
	});
}
