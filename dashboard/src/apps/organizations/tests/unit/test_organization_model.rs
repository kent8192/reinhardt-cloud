use chrono::Utc;
use rstest::rstest;
use serde_json;

use crate::apps::organizations::models::Organization;

#[rstest]
fn organization_serializes_and_deserializes_roundtrip() {
	// Arrange
	let org = Organization {
		id: Some(42),
		slug: "acme".to_string(),
		name: "Acme Corporation".to_string(),
		created_at: Utc::now(),
		updated_at: Utc::now(),
	};

	// Act
	let json = serde_json::to_string(&org).expect("serialize");
	let roundtripped: Organization = serde_json::from_str(&json).expect("deserialize");

	// Assert
	assert_eq!(roundtripped.id, Some(42));
	assert_eq!(roundtripped.slug, "acme");
	assert_eq!(roundtripped.name, "Acme Corporation");
}

#[rstest]
fn organization_id_is_optional_for_inserts() {
	// Arrange
	let org = Organization {
		id: None,
		slug: "newcorp".to_string(),
		name: "New Corp".to_string(),
		created_at: Utc::now(),
		updated_at: Utc::now(),
	};

	// Act
	let json = serde_json::to_string(&org).expect("serialize");

	// Assert -- `id: null` must be present so the ORM treats it as auto-increment
	assert!(json.contains("\"id\":null"), "got: {json}");
}
