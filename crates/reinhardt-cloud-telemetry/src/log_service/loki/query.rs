//! Pure LogQL builder turning a `LogFilter` into a Loki query string.
//!
//! Kept free of any I/O so it is unit-testable without a Loki instance. The
//! `since`/`until` filter fields are intentionally NOT part of the LogQL string
//! — they become the `start`/`end` query parameters of `query_range`.

use reinhardt_cloud_types::log::{LogFilter, LogLevel};

/// Levels at or above the given severity, in Loki `level` label regex form.
fn levels_at_or_above(level: LogLevel) -> &'static str {
	match level {
		LogLevel::Debug => "debug|info|warn|error",
		LogLevel::Info => "info|warn|error",
		LogLevel::Warn => "warn|error",
		LogLevel::Error => "error",
	}
}

/// Escape a user-supplied search term for safe embedding in a LogQL regex.
///
/// LogQL line filters use RE2; backslash-escape the characters that have special
/// meaning so a search string is matched literally unless it intentionally uses
/// regex syntax.
fn escape_regex(term: &str) -> String {
	let specials: &str = r"\.^$|?*+()[]{}";
	let mut out = String::with_capacity(term.len());
	for ch in term.chars() {
		if specials.contains(ch) {
			out.push('\\');
		}
		out.push(ch);
	}
	out
}

/// Escape a value for a double-quoted LogQL string literal.
fn escape_logql_string_literal(value: &str) -> String {
	let mut out = String::with_capacity(value.len());
	for ch in value.chars() {
		match ch {
			'"' => out.push_str(r#"\""#),
			'\\' => out.push_str(r#"\\"#),
			'\n' => out.push_str(r#"\n"#),
			'\r' => out.push_str(r#"\r"#),
			'\t' => out.push_str(r#"\t"#),
			ch if ch.is_control() => out.push_str(&format!(r#"\x{:02x}"#, ch as u32)),
			ch => out.push(ch),
		}
	}
	out
}

/// Build a LogQL query string for Loki `query_range` / `tail` from a filter.
///
/// `source` maps to the `app` label (the project name written by Promtail);
/// `min_level` narrows the `level` label; `search` becomes a regex line filter;
/// `deployment_id` adds a `deployment_id` label matcher only when set.
pub(crate) fn build_logql(filter: &LogFilter) -> String {
	let mut selectors: Vec<String> = Vec::new();

	// The `app` selector is the primary key. Default to a broad match when unset.
	match &filter.source {
		Some(app) => selectors.push(format!(r#"app="{}""#, escape_logql_string_literal(app))),
		None => selectors.push(r#"app=~".+""#.to_string()),
	}
	if let Some(deployment_id) = &filter.deployment_id {
		selectors.push(format!(
			r#"deployment_id="{}""#,
			escape_logql_string_literal(deployment_id)
		));
	}
	if let Some(min_level) = filter.min_level {
		selectors.push(format!(r#"level=~"{}""#, levels_at_or_above(min_level)));
	}

	let mut query = format!("{{{}}}", selectors.join(","));

	if let Some(search) = &filter.search {
		let escaped_regex = escape_regex(search);
		query.push_str(&format!(
			"|~\"{}\"",
			escape_logql_string_literal(&escaped_regex)
		));
	}
	query
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::TimeZone;
	use reinhardt_cloud_types::log::{LogFilter, LogLevel};
	use rstest::rstest;

	#[rstest]
	fn empty_filter_uses_broad_app_selector() {
		// Arrange
		let filter = LogFilter::default();

		// Act
		let q = build_logql(&filter);

		// Assert
		assert_eq!(q, r#"{app=~".+"}"#);
	}

	#[rstest]
	fn source_becomes_app_label() {
		// Arrange
		let filter = LogFilter {
			source: Some("my-project".to_string()),
			..Default::default()
		};

		// Act
		let q = build_logql(&filter);

		// Assert
		assert_eq!(q, r#"{app="my-project"}"#);
	}

	#[rstest]
	fn min_level_warn_expands_to_level_regex() {
		// Arrange
		let filter = LogFilter {
			source: Some("p".to_string()),
			min_level: Some(LogLevel::Warn),
			..Default::default()
		};

		// Act
		let q = build_logql(&filter);

		// Assert
		assert_eq!(q, r#"{app="p",level=~"warn|error"}"#);
	}

	#[rstest]
	fn search_is_appended_as_escaped_regex_line_filter() {
		// Arrange
		let filter = LogFilter {
			source: Some("p".to_string()),
			search: Some("rate (limit)?".to_string()),
			..Default::default()
		};

		// Act
		let q = build_logql(&filter);

		// Assert — parens and the `?` quantifier are escaped; alphanumerics are
		// left literal so the search matches as plain text in RE2.
		assert_eq!(q, r#"{app="p"}|~"rate \\(limit\\)\\?""#);
	}

	#[rstest]
	fn label_values_escape_quotes_backslashes_and_newlines() {
		// Arrange
		let filter = LogFilter {
			source: Some("my\"project\\name\nnext".to_string()),
			deployment_id: Some("deploy\"7\\x".to_string()),
			..Default::default()
		};

		// Act
		let q = build_logql(&filter);

		// Assert
		assert_eq!(
			q,
			r#"{app="my\"project\\name\nnext",deployment_id="deploy\"7\\x"}"#
		);
	}

	#[rstest]
	fn search_string_escapes_regex_and_logql_literal_characters() {
		// Arrange
		let filter = LogFilter {
			source: Some("p".to_string()),
			search: Some("quote \" slash \\ newline\n bell\u{0007}".to_string()),
			..Default::default()
		};

		// Act
		let q = build_logql(&filter);

		// Assert
		assert_eq!(q, r#"{app="p"}|~"quote \" slash \\\\ newline\n bell\x07""#);
	}

	#[rstest]
	fn deployment_id_label_only_when_set() {
		// Arrange
		let filter = LogFilter {
			source: Some("p".to_string()),
			deployment_id: Some("deploy-7".to_string()),
			..Default::default()
		};

		// Act
		let q = build_logql(&filter);

		// Assert
		assert_eq!(q, r#"{app="p",deployment_id="deploy-7"}"#);
	}

	#[rstest]
	fn all_fields_combined() {
		// Arrange
		let filter = LogFilter {
			source: Some("p".to_string()),
			min_level: Some(LogLevel::Error),
			search: Some("oom".to_string()),
			deployment_id: Some("d1".to_string()),
			..Default::default()
		};

		// Act
		let q = build_logql(&filter);

		// Assert
		assert_eq!(q, r#"{app="p",deployment_id="d1",level=~"error"}|~"oom""#);
	}

	#[rstest]
	fn since_until_do_not_appear_in_logql() {
		// Arrange — since/until are query_range params, not part of LogQL.
		let filter = LogFilter {
			source: Some("p".to_string()),
			since: Some(chrono::Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()),
			until: Some(chrono::Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap()),
			..Default::default()
		};

		// Act
		let q = build_logql(&filter);

		// Assert
		assert_eq!(q, r#"{app="p"}"#);
	}
}
