use rstest::rstest;

use crate::apps::organizations::roles::MembershipRole;

#[rstest]
#[case(MembershipRole::Owner, MembershipRole::Owner, true)]
#[case(MembershipRole::Owner, MembershipRole::Admin, true)]
#[case(MembershipRole::Owner, MembershipRole::Developer, true)]
#[case(MembershipRole::Owner, MembershipRole::Viewer, true)]
#[case(MembershipRole::Admin, MembershipRole::Owner, false)]
#[case(MembershipRole::Admin, MembershipRole::Admin, true)]
#[case(MembershipRole::Admin, MembershipRole::Developer, true)]
#[case(MembershipRole::Admin, MembershipRole::Viewer, true)]
#[case(MembershipRole::Developer, MembershipRole::Admin, false)]
#[case(MembershipRole::Developer, MembershipRole::Developer, true)]
#[case(MembershipRole::Developer, MembershipRole::Viewer, true)]
#[case(MembershipRole::Viewer, MembershipRole::Developer, false)]
#[case(MembershipRole::Viewer, MembershipRole::Viewer, true)]
fn membership_role_can_respects_hierarchy(
	#[case] held: MembershipRole,
	#[case] required: MembershipRole,
	#[case] expected: bool,
) {
	// Arrange / Act
	let actual = held.can(required);

	// Assert
	assert_eq!(actual, expected);
}

#[rstest]
#[case("owner", Some(MembershipRole::Owner))]
#[case("admin", Some(MembershipRole::Admin))]
#[case("developer", Some(MembershipRole::Developer))]
#[case("viewer", Some(MembershipRole::Viewer))]
#[case("OWNER", Some(MembershipRole::Owner))]
#[case("not_a_role", None)]
#[case("", None)]
fn membership_role_parse_from_db_string(
	#[case] input: &str,
	#[case] expected: Option<MembershipRole>,
) {
	// Arrange / Act
	let actual = MembershipRole::from_db_str(input);

	// Assert
	assert_eq!(actual, expected);
}

#[rstest]
#[case(MembershipRole::Owner, "owner")]
#[case(MembershipRole::Admin, "admin")]
#[case(MembershipRole::Developer, "developer")]
#[case(MembershipRole::Viewer, "viewer")]
fn membership_role_to_db_string(
	#[case] role: MembershipRole,
	#[case] expected: &str,
) {
	// Arrange / Act
	let actual = role.as_db_str();

	// Assert
	assert_eq!(actual, expected);
}
