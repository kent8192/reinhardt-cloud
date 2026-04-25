use rstest::rstest;

use crate::apps::organizations::roles::{
	is_reserved_slug, sanitize_username_to_slug, validate_slug,
};

#[rstest]
#[case("alice", "alice")]
#[case("Alice_Smith", "alice-smith")]
#[case("user.name", "user-name")]
#[case("MIXED-Case", "mixed-case")]
#[case("trailing-dash-", "trailing-dash")]
#[case("---leading", "leading")]
#[case("a", "a")]
fn sanitize_produces_valid_dns_label(#[case] input: &str, #[case] expected: &str) {
	// Arrange / Act
	let actual = sanitize_username_to_slug(input);

	// Assert
	assert_eq!(actual, expected);
	assert!(validate_slug(&actual).is_ok(), "sanitized output must validate");
}

#[rstest]
fn sanitize_truncates_to_63_characters() {
	// Arrange — input longer than 63 chars
	let input = "a".repeat(80);

	// Act
	let actual = sanitize_username_to_slug(&input);

	// Assert
	assert!(actual.len() <= 63);
	assert!(validate_slug(&actual).is_ok());
}

#[rstest]
fn sanitize_empty_falls_back_to_user_prefix() {
	// Arrange
	let input = "###";

	// Act
	let actual = sanitize_username_to_slug(input);

	// Assert
	assert!(actual.starts_with("user-"));
	assert!(validate_slug(&actual).is_ok());
}

#[rstest]
fn sanitize_leading_digit_falls_back_to_user_prefix() {
	// Arrange — collapsed slug starts with a digit, which is invalid
	let input = "123abc";

	// Act
	let actual = sanitize_username_to_slug(input);

	// Assert
	assert!(actual.starts_with("user-"));
	assert!(validate_slug(&actual).is_ok());
}

#[rstest]
#[case("acme", true)]
#[case("a-b-c", true)]
#[case("a", true)]
#[case("0acme", false)] // must start with letter
#[case("acme-", false)] // must end with alnum
#[case("ACME", false)] // must be lowercase
#[case("a_b", false)] // underscores not allowed
#[case("", false)]
fn validate_slug_enforces_dns_1123_label(#[case] input: &str, #[case] expected_ok: bool) {
	// Arrange / Act
	let actual = validate_slug(input);

	// Assert
	assert_eq!(actual.is_ok(), expected_ok, "input={input}");
}

#[rstest]
#[case("kube-system", true)]
#[case("kube-public", true)]
#[case("default", true)]
#[case("reinhardt-cloud-system", true)]
#[case("system", true)]
#[case("admin", true)]
#[case("api", true)]
#[case("dashboard", true)]
#[case("acme", false)]
#[case("", true)] // empty also reserved (rejected by validate_slug separately)
fn is_reserved_slug_blocks_known_names(#[case] input: &str, #[case] expected: bool) {
	// Arrange / Act
	let actual = is_reserved_slug(input);

	// Assert
	assert_eq!(actual, expected);
}
