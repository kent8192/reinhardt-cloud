use chrono::Utc;
use rstest::rstest;
use serde_json;
use uuid::Uuid;

use crate::apps::organizations::models::Organization;

#[rstest]
fn organization_serializes_and_deserializes_roundtrip() {
	// Arrange
	let creator_id = Uuid::new_v4();
	let org = Organization {
		id: Some(42),
		slug: "acme".to_string(),
		name: "Acme Corporation".to_string(),
		created_by: creator_id,
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
	assert_eq!(roundtripped.created_by, creator_id);
}

#[rstest]
fn organization_id_is_optional_for_inserts() {
	// Arrange
	let org = Organization {
		id: None,
		slug: "newcorp".to_string(),
		name: "New Corp".to_string(),
		created_by: Uuid::new_v4(),
		created_at: Utc::now(),
		updated_at: Utc::now(),
	};

	// Act
	let json = serde_json::to_string(&org).expect("serialize");

	// Assert -- `id: null` must be present so the ORM treats it as auto-increment
	assert!(json.contains("\"id\":null"), "got: {json}");
}

/// Verify `created_by` is preserved through serde round-trip and matches the
/// originating user UUID. This guards the audit-trail invariant: any user
/// who creates an organization must be permanently attributable as its
/// creator (refs #435).
#[rstest]
fn organization_created_by_round_trip_preserves_uuid() {
	// Arrange -- simulate a user-created organization
	let creator_id = Uuid::new_v4();
	let org = Organization {
		id: Some(1),
		slug: "audit-corp".to_string(),
		name: "Audit Corp".to_string(),
		created_by: creator_id,
		created_at: Utc::now(),
		updated_at: Utc::now(),
	};

	// Act -- serialize then deserialize, mimicking the DB write/read path
	let json = serde_json::to_string(&org).expect("serialize");
	let roundtripped: Organization = serde_json::from_str(&json).expect("deserialize");

	// Assert -- the creator UUID survives intact and is present in the wire format
	assert_eq!(
		roundtripped.created_by, creator_id,
		"created_by must round-trip without mutation",
	);
	assert!(
		json.contains(&format!("\"created_by\":\"{creator_id}\"")),
		"created_by must be serialized as a UUID string; got: {json}",
	);
}
