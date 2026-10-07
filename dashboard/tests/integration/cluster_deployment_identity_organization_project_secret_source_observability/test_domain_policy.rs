use chrono::{TimeZone, Utc};
use cloud_dashboard::apps::cluster::services::{ClusterCapabilities, ClusterError};
use cloud_dashboard::apps::deployment::services::{
	OperationError, OperationProgress, OperationState,
};
use cloud_dashboard::apps::identity::services::{SessionError, SessionFacts};
use cloud_dashboard::apps::observability::services::LogWindow;
use cloud_dashboard::apps::organization::services::{
	Action, AuthorizationError, AuthorizationService, EnvironmentGrant, Membership, Role,
};
use cloud_dashboard::apps::project::services::{EnvironmentKind, EnvironmentTarget};
use cloud_dashboard::apps::secret::services::{SecretError, SecretReference};
use cloud_dashboard::apps::source::services::{SourceError, SourceRevision};
use rstest::rstest;
use uuid::Uuid;

#[rstest]
#[case(Role::Viewer, Action::ReadApplication, false, Ok(()))]
#[case(
	Role::Viewer,
	Action::ChangeApplication,
	true,
	Err(AuthorizationError::Forbidden)
)]
#[case(
	Role::Developer,
	Action::ChangeApplication,
	false,
	Err(AuthorizationError::Forbidden)
)]
#[case(Role::Developer, Action::ChangeApplication, true, Ok(()))]
#[case(
	Role::Developer,
	Action::ApproveConfiguration,
	true,
	Err(AuthorizationError::Forbidden)
)]
#[case(
	Role::Developer,
	Action::ChangeSecret,
	true,
	Err(AuthorizationError::Forbidden)
)]
#[case(
	Role::Developer,
	Action::DeleteResource,
	true,
	Err(AuthorizationError::Forbidden)
)]
#[case(
	Role::Developer,
	Action::ReadAudit,
	true,
	Err(AuthorizationError::Forbidden)
)]
#[case(
	Role::Admin,
	Action::ManageOwnership,
	false,
	Err(AuthorizationError::Forbidden)
)]
#[case(Role::Admin, Action::ApproveConfiguration, false, Ok(()))]
#[case(Role::Owner, Action::ManageOwnership, false, Ok(()))]
fn production_authorization(
	#[case] role: Role,
	#[case] action: Action,
	#[case] granted: bool,
	#[case] expected: Result<(), AuthorizationError>,
) {
	// Arrange
	let user = Uuid::from_u128(1);
	let organization = Uuid::from_u128(2);
	let environment = Uuid::from_u128(3);
	let memberships = [Membership {
		user_id: user,
		organization_id: organization,
		role,
	}];
	let grants = if granted {
		vec![EnvironmentGrant {
			user_id: user,
			organization_id: organization,
			environment_id: environment,
		}]
	} else {
		Vec::new()
	};
	let service = AuthorizationService::new(&memberships, &grants);
	// Act
	let actual = service.authorize(
		user,
		organization,
		Some((environment, EnvironmentKind::Production)),
		action,
	);
	// Assert
	assert_eq!(actual, expected);
}

#[rstest]
fn membership_and_grants_cannot_cross_organizations() {
	// Arrange
	let user = Uuid::from_u128(1);
	let memberships = [Membership {
		user_id: user,
		organization_id: Uuid::from_u128(2),
		role: Role::Owner,
	}];
	let service = AuthorizationService::new(&memberships, &[]);
	// Act
	let actual = service.authorize(user, Uuid::from_u128(3), None, Action::ReadApplication);
	// Assert
	assert_eq!(actual, Err(AuthorizationError::NotAMember));
}

#[rstest]
fn the_last_owner_cannot_leave() {
	// Arrange
	let user = Uuid::from_u128(1);
	let organization = Uuid::from_u128(2);
	let memberships = [Membership {
		user_id: user,
		organization_id: organization,
		role: Role::Owner,
	}];
	let service = AuthorizationService::new(&memberships, &[]);
	// Act
	let actual = service.ensure_owner_can_leave(user, organization);
	// Assert
	assert_eq!(actual, Err(AuthorizationError::LastOwner));
}

#[rstest]
fn deadline_expiry_keeps_the_environment_blocked_until_reconciliation() {
	// Arrange
	let mut operation = OperationProgress {
		id: Uuid::from_u128(1),
		environment_id: Uuid::from_u128(2),
		environment_version: 3,
		state: OperationState::Applying,
	};
	// Act
	operation.deadline_exceeded().unwrap();
	// Assert
	assert_eq!(operation.state, OperationState::Uncertain);
	assert!(operation.blocks_environment());
	assert_eq!(operation.cancel(), Err(OperationError::UnsafeCancellation));
	assert_eq!(
		operation.reconcile(operation.id, 2, OperationState::Succeeded),
		Err(OperationError::StaleResult)
	);
	assert_eq!(operation.state, OperationState::Uncertain);
	operation
		.reconcile(operation.id, 3, OperationState::Succeeded)
		.unwrap();
	assert!(!operation.blocks_environment());
}

#[rstest]
fn a_different_operation_cannot_complete_the_current_operation() {
	// Arrange
	let mut operation = OperationProgress {
		id: Uuid::from_u128(1),
		environment_id: Uuid::from_u128(2),
		environment_version: 3,
		state: OperationState::Verifying,
	};
	// Act
	let actual = operation.reconcile(Uuid::from_u128(4), 3, OperationState::Succeeded);
	// Assert
	assert_eq!(actual, Err(OperationError::StaleResult));
	assert_eq!(operation.state, OperationState::Verifying);
}

#[rstest]
#[case(false, true, 0, Ok(Uuid::from_u128(1)))]
#[case(true, true, 0, Err(SessionError::Revoked))]
#[case(false, false, 0, Err(SessionError::InactiveUser))]
#[case(false, true, 60, Err(SessionError::Expired))]
fn sessions_use_current_revocation_and_expiry(
	#[case] revoked: bool,
	#[case] active: bool,
	#[case] elapsed: i64,
	#[case] expected: Result<Uuid, SessionError>,
) {
	// Arrange
	let session = SessionFacts {
		user_id: Uuid::from_u128(1),
		expires_at: Utc.timestamp_opt(60, 0).unwrap(),
		revoked,
		user_active: active,
	};
	// Act
	let actual = session.authenticate(Utc.timestamp_opt(elapsed, 0).unwrap());
	// Assert
	assert_eq!(actual, expected);
}

#[rstest]
#[case("main", false, Err(SourceError::MutableRevision))]
#[case(
	"0123456789abcdef0123456789abcdef01234567",
	true,
	Err(SourceError::ExternalFork)
)]
#[case("0123456789abcdef0123456789abcdef01234567", false, Ok(()))]
fn source_execution_requires_fixed_and_trusted_inputs(
	#[case] commit: &str,
	#[case] external: bool,
	#[case] expected: Result<(), SourceError>,
) {
	// Arrange
	let revision = SourceRevision {
		repository: "https://github.com/example/app".into(),
		commit: commit.into(),
	};
	// Act
	let actual = revision.validate(external);
	// Assert
	assert_eq!(actual, expected);
}

#[rstest]
fn log_windows_bound_rows_and_utf8_bytes() {
	// Arrange
	let mut window = LogWindow::new(2, 8);
	// Act
	window.push("first".into());
	window.push("日本".into());
	// Assert
	assert_eq!(
		window.rows().iter().map(String::as_str).collect::<Vec<_>>(),
		vec!["日本"]
	);
	assert_eq!(window.retained_bytes(), 6);
	assert!(window.continuity_lost);
}

#[rstest]
fn secret_versions_cannot_be_supplied_after_revocation_or_to_another_environment() {
	// Arrange
	let reference = SecretReference {
		environment_id: Uuid::from_u128(1),
		version_id: Uuid::from_u128(2),
	};
	// Act
	let revoked = reference.validate_supply(reference.environment_id, true);
	let other = reference.validate_supply(Uuid::from_u128(3), false);
	// Assert
	assert_eq!(revoked, Err(SecretError::Revoked));
	assert_eq!(other, Err(SecretError::WrongEnvironment));
}

#[rstest]
fn environment_namespace_uses_immutable_identity() {
	// Arrange
	let environment = EnvironmentTarget {
		organization_id: Uuid::from_u128(1),
		project_id: Uuid::from_u128(2),
		environment_id: Uuid::from_u128(3),
		kind: EnvironmentKind::Staging,
	};
	// Act
	let namespace = environment.namespace();
	// Assert
	assert_eq!(namespace, "env-00000000000000000000000000000003");
}

#[rstest]
fn registration_and_scaling_validate_cluster_capabilities() {
	// Arrange
	let cluster = ClusterCapabilities {
		rootless_builds: true,
		ingress: true,
		dns: true,
		namespace_issuer: false,
		cpu_autoscaling: false,
	};
	// Act
	let registration = cluster.validate_registration();
	let scaling = cluster.admit_scale(2, 4, true);
	// Assert
	assert_eq!(registration, Err(ClusterError::MissingHttps));
	assert_eq!(scaling, Err(ClusterError::UnsupportedAutoscaling));
	assert_eq!(
		cluster.admit_scale(5, 4, false),
		Err(ClusterError::ReplicaLimit)
	);
}
