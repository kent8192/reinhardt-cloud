use chrono::Utc;
use rstest::rstest;
use serde_json;
use uuid::Uuid;

use crate::apps::organizations::models::OrganizationMembership;
use crate::apps::organizations::roles::MembershipRole;

#[rstest]
fn membership_serializes_with_role_string() {
	// Arrange
	let user_id = Uuid::new_v4();
	let mut m = OrganizationMembership::build()
		.organization(42)
		.user(user_id)
		.role(MembershipRole::Owner.as_db_str().to_string())
		.finish();
	m.id = Some(7);
	m.created_at = Utc::now();

	// Act
	let json = serde_json::to_string(&m).expect("serialize");
	let roundtripped: OrganizationMembership = serde_json::from_str(&json).expect("deserialize");

	// Assert
	assert_eq!(*roundtripped.organization_id(), 42);
	assert_eq!(*roundtripped.user_id(), user_id);
	assert_eq!(roundtripped.role, "owner");
	assert!(json.contains("\"role\":\"owner\""));
}
