//! Unit tests of the sign-up policy decision (SR-19).

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use rstest::rstest;

use crate::apps::accounts::services::server::sign_up_policy::{
	AdmitReason, DenyReason, MembershipError, OrganizationMembership, SignUpDecision, SignUpPolicy,
	SignUpPolicyError,
};

/// Membership lookup with a fixed answer that counts how often it is asked.
struct FixedMembership {
	answer: Result<Vec<i64>, MembershipError>,
	calls: AtomicUsize,
}

impl FixedMembership {
	fn answering(organization_ids: Vec<i64>) -> Self {
		Self {
			answer: Ok(organization_ids),
			calls: AtomicUsize::new(0),
		}
	}

	fn failing() -> Self {
		Self {
			answer: Err(MembershipError),
			calls: AtomicUsize::new(0),
		}
	}

	fn calls(&self) -> usize {
		self.calls.load(Ordering::SeqCst)
	}
}

#[async_trait]
impl OrganizationMembership for FixedMembership {
	async fn organization_ids(&self, _github_user_id: i64) -> Result<Vec<i64>, MembershipError> {
		self.calls.fetch_add(1, Ordering::SeqCst);
		self.answer.clone()
	}
}

fn allowlist(users: &str, organizations: &str) -> SignUpPolicy {
	SignUpPolicy::from_settings("allowlist", users, organizations).unwrap()
}

#[rstest]
#[case::empty("")]
#[case::missing_value("   ")]
#[case::unknown("everyone")]
#[case::misspelled("alowlist")]
#[case::wrong_separator("invite-only")]
fn sr_19_unrecognized_or_missing_policy_resolves_to_invite_only(#[case] name: &str) {
	// Arrange / Act
	let policy = SignUpPolicy::from_settings(name, "", "").unwrap();

	// Assert
	assert_eq!(policy, SignUpPolicy::InviteOnly);
}

#[rstest]
#[case("open", SignUpPolicy::Open)]
#[case("OPEN", SignUpPolicy::Open)]
#[case("  invite_only ", SignUpPolicy::InviteOnly)]
fn sr_19_known_policy_names_are_recognized(#[case] name: &str, #[case] expected: SignUpPolicy) {
	// Arrange / Act
	let policy = SignUpPolicy::from_settings(name, "", "").unwrap();

	// Assert
	assert_eq!(policy, expected);
}

#[rstest]
#[tokio::test]
async fn sr_19_open_admits_any_identity_without_consulting_github() {
	// Arrange
	let membership = FixedMembership::failing();

	// Act
	let decision = SignUpPolicy::Open.decide(7, &membership).await;

	// Assert
	assert_eq!(decision, SignUpDecision::Admit(AdmitReason::Open));
	assert_eq!(membership.calls(), 0);
}

#[rstest]
#[tokio::test]
async fn sr_19_invite_only_denies_an_unknown_identity() {
	// Arrange
	let membership = FixedMembership::answering(vec![1]);

	// Act
	let decision = SignUpPolicy::InviteOnly.decide(7, &membership).await;

	// Assert
	assert_eq!(decision, SignUpDecision::Deny(DenyReason::NotInvited));
	assert_eq!(membership.calls(), 0);
}

#[rstest]
#[tokio::test]
async fn sr_19_allowlist_admits_a_listed_user_id_without_a_github_lookup() {
	// Arrange
	let policy = allowlist("7, 8", "100");
	let membership = FixedMembership::failing();

	// Act
	let decision = policy.decide(8, &membership).await;

	// Assert
	assert_eq!(
		decision,
		SignUpDecision::Admit(AdmitReason::AllowlistedUser)
	);
	assert_eq!(membership.calls(), 0);
}

#[rstest]
#[tokio::test]
async fn sr_19_allowlist_admits_a_member_of_a_listed_organization_by_numeric_id() {
	// Arrange
	let policy = allowlist("", "100,200");
	let membership = FixedMembership::answering(vec![5, 200]);

	// Act
	let decision = policy.decide(7, &membership).await;

	// Assert
	assert_eq!(
		decision,
		SignUpDecision::Admit(AdmitReason::AllowlistedOrganization)
	);
	assert_eq!(membership.calls(), 1);
}

#[rstest]
#[tokio::test]
async fn sr_19_allowlist_denies_when_no_organization_matches() {
	// Arrange
	let policy = allowlist("1", "100");
	let membership = FixedMembership::answering(vec![5, 6]);

	// Act
	let decision = policy.decide(7, &membership).await;

	// Assert
	assert_eq!(decision, SignUpDecision::Deny(DenyReason::NotAllowlisted));
}

#[rstest]
#[tokio::test]
async fn sr_19_allowlist_without_organizations_never_asks_github() {
	// Arrange
	let policy = allowlist("1", "");
	let membership = FixedMembership::answering(vec![100]);

	// Act
	let decision = policy.decide(7, &membership).await;

	// Assert
	assert_eq!(decision, SignUpDecision::Deny(DenyReason::NotAllowlisted));
	assert_eq!(membership.calls(), 0);
}

#[rstest]
#[tokio::test]
async fn sr_19_allowlist_fails_closed_when_membership_cannot_be_verified() {
	// Arrange
	let policy = allowlist("", "100");
	let membership = FixedMembership::failing();

	// Act
	let decision = policy.decide(7, &membership).await;

	// Assert
	assert_eq!(
		decision,
		SignUpDecision::Deny(DenyReason::MembershipUnverified)
	);
}

#[rstest]
#[case::login("my-org")]
#[case::negative("-4")]
#[case::zero("0")]
#[case::float("1.5")]
#[case::overflow("99999999999999999999")]
fn sr_19_allowlist_entries_must_be_numeric_ids(#[case] entry: &str) {
	// Arrange / Act
	let by_user = SignUpPolicy::from_settings("allowlist", entry, "");
	let by_organization = SignUpPolicy::from_settings("invite_only", "", &format!("1,{entry}"));

	// Assert
	let expected = SignUpPolicyError::InvalidAllowlistEntry {
		entry: entry.to_owned(),
	};
	assert_eq!(by_user.unwrap_err(), expected);
	assert_eq!(by_organization.unwrap_err(), expected);
}

#[rstest]
#[case(DenyReason::NotInvited, "not_invited")]
#[case(DenyReason::NotAllowlisted, "not_allowlisted")]
#[case(DenyReason::MembershipUnverified, "membership_unverified")]
fn sr_19_denial_reason_codes_are_stable_audit_codes(
	#[case] reason: DenyReason,
	#[case] code: &str,
) {
	// Arrange / Act / Assert
	assert_eq!(reason.code(), code);
}
