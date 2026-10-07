use crate::fixture::CloudFixture;
use cloud_dashboard::apps::cluster::models::{Cluster, EnvironmentAllocation};
use cloud_dashboard::apps::deployment::models::{DispatchRecord, Operation};
use cloud_dashboard::apps::deployment::persistence::{
	AcceptanceError, accept_operation, mark_deadline_uncertain, pending_for_cluster,
	reconcile_result, record_receipt,
};
use cloud_dashboard::apps::deployment::services::{
	OperationProgress, OperationRequest, OperationState, RuntimeChange,
};
use cloud_dashboard::apps::identity::models::UserAccount;
use cloud_dashboard::apps::observability::models::AuditRecord;
use cloud_dashboard::apps::organization::models::{EnvironmentGrant, Membership, Organization};
use cloud_dashboard::apps::project::models::{Environment, Project};
use cloud_dashboard::apps::project::services::{CpuAutoscaling, DesiredRuntime};
use reinhardt::db::orm::{DatabaseConnection, Model, connection::DatabaseConnectionLease};
use reinhardt::query::{PostgresQueryBuilder, Query, QueryBuilder};
use rstest::rstest;
use uuid::Uuid;

struct OperationFixture {
	cloud: CloudFixture,
	lease: DatabaseConnectionLease,
	user: Uuid,
	organization: Uuid,
	environment: Uuid,
	cluster: Uuid,
}

impl OperationFixture {
	async fn new(role: &str, kind: &str) -> Self {
		let cloud = CloudFixture::new().await;
		cloud.run(&["migrate"]).await;
		let backend = reinhardt::db::backends::DatabaseConnection::connect_postgres_with_pool_size(
			&cloud.database_url(),
			Some(4),
		)
		.await
		.unwrap();
		let lease = DatabaseConnectionLease::register(backend).unwrap();
		let mut connection = lease.handle();
		let user = UserAccount::new()
			.email("operator@example.test")
			.password_hash("disabled-password-entry")
			.active(true)
			.platform_admin(false)
			.finish();
		let user = UserAccount::objects()
			.create_with_conn(&mut connection, &user)
			.await
			.unwrap();
		let organization = Organization::new().display_name("Operation Tests").finish();
		let organization = Organization::objects()
			.create_with_conn(&mut connection, &organization)
			.await
			.unwrap();
		let membership = Membership::new()
			.organization(organization.id)
			.user(user.id)
			.role(role)
			.finish();
		Membership::objects()
			.create_with_conn(&mut connection, &membership)
			.await
			.unwrap();
		let project = Project::new()
			.organization(organization.id)
			.display_name("Test App")
			.repository_url("https://github.com/example/app")
			.finish();
		let project = Project::objects()
			.create_with_conn(&mut connection, &project)
			.await
			.unwrap();
		let environment = Environment::new()
			.organization(organization.id)
			.project(project.id)
			.kind(kind)
			.version(0)
			.desired_runtime("{}")
			.finish();
		let environment = Environment::objects()
			.create_with_conn(&mut connection, &environment)
			.await
			.unwrap();
		let cluster = Cluster::new()
			.display_name("Isolated Cluster")
			.registered(true)
			.rootless_builds(true)
			.ingress(true)
			.dns(true)
			.namespace_issuer(true)
			.cpu_autoscaling(true)
			.finish();
		let cluster = Cluster::objects()
			.create_with_conn(&mut connection, &cluster)
			.await
			.unwrap();
		let allocation = EnvironmentAllocation::new()
			.environment(environment.id)
			.cluster(cluster.id)
			.replica_limit(3)
			.finish();
		EnvironmentAllocation::objects()
			.create_with_conn(&mut connection, &allocation)
			.await
			.unwrap();
		Self {
			cloud,
			lease,
			user: user.id,
			organization: organization.id,
			environment: environment.id,
			cluster: cluster.id,
		}
	}

	fn request(&self, key: &str, version: i64, change: RuntimeChange) -> OperationRequest {
		OperationRequest {
			organization_id: self.organization,
			environment_id: self.environment,
			expected_version: version,
			idempotency_key: key.into(),
			change,
		}
	}

	async fn environment(&self, connection: &mut DatabaseConnection) -> Environment {
		Environment::objects()
			.filter(Environment::field_id().eq(self.environment))
			.all_with_db(connection)
			.await
			.unwrap()
			.remove(0)
	}
}

#[rstest]
#[tokio::test]
async fn concurrent_acceptance_replays_durable_identity_and_fences_late_results() {
	// Arrange
	let fixture = OperationFixture::new("owner", "staging").await;
	let connection = fixture.lease.handle();
	let request = fixture.request("restart-one", 0, RuntimeChange::Restart);
	// Act
	let (first, duplicate) = tokio::join!(
		accept_operation(&connection, fixture.user, &request),
		accept_operation(&connection, fixture.user, &request),
	);
	let first = first.unwrap();
	let duplicate = duplicate.unwrap();
	// Assert
	assert_eq!(first, duplicate);
	assert_eq!(first.state, OperationState::Queued);
	assert_eq!(first.environment_version, 1);
	let mut connection = connection;
	assert_eq!(
		Operation::objects()
			.all()
			.all_with_db(&mut connection)
			.await
			.unwrap()
			.len(),
		1
	);
	assert_eq!(
		DispatchRecord::objects()
			.all()
			.all_with_db(&mut connection)
			.await
			.unwrap()
			.len(),
		1
	);
	assert_eq!(
		AuditRecord::objects()
			.all()
			.all_with_db(&mut connection)
			.await
			.unwrap()
			.len(),
		1
	);
	let environment = fixture.environment(&mut connection).await;
	assert_eq!(environment.version, 1);
	assert_eq!(
		serde_json::from_str::<DesiredRuntime>(&environment.desired_runtime)
			.unwrap()
			.restart_nonce,
		Some(first.id)
	);

	// Arrange: open a separate connection owner, with no in-process queue state.
	let backend = reinhardt::db::backends::DatabaseConnection::connect_postgres_with_pool_size(
		&fixture.cloud.database_url(),
		Some(2),
	)
	.await
	.unwrap();
	let resumed = DatabaseConnectionLease::register(backend).unwrap();
	let mut resumed_connection = resumed.handle();
	// Act
	let pending = pending_for_cluster(&mut resumed_connection, fixture.cluster)
		.await
		.unwrap();
	let redelivery = pending_for_cluster(&mut resumed_connection, fixture.cluster)
		.await
		.unwrap();
	// Assert
	assert_eq!(pending, redelivery);
	assert_eq!(pending.len(), 1);
	assert_eq!(pending[0].operation_id, first.id);
	assert_eq!(pending[0].environment_version, 1);
	assert_eq!(pending[0].desired_runtime.restart_nonce, Some(first.id));
	assert_eq!(
		pending_for_cluster(&mut resumed_connection, Uuid::new_v4())
			.await
			.unwrap(),
		vec![]
	);
	assert!(matches!(
		record_receipt(&resumed_connection, Uuid::new_v4(), &first).await,
		Err(AcceptanceError::StaleResult)
	));
	assert!(matches!(
		reconcile_result(
			&resumed_connection,
			fixture.cluster,
			&OperationProgress {
				state: OperationState::Applying,
				..first.clone()
			}
		)
		.await,
		Err(AcceptanceError::UnsafeTransition)
	));

	// Act
	record_receipt(&resumed_connection, fixture.cluster, &first)
		.await
		.unwrap();
	record_receipt(&resumed_connection, fixture.cluster, &first)
		.await
		.unwrap();
	for state in [
		OperationState::Applying,
		OperationState::Verifying,
		OperationState::Succeeded,
	] {
		let result = OperationProgress {
			state,
			..first.clone()
		};
		assert_eq!(
			reconcile_result(&resumed_connection, fixture.cluster, &result)
				.await
				.unwrap(),
			result
		);
	}
	let completed = accept_operation(&resumed_connection, fixture.user, &request)
		.await
		.unwrap();
	let mismatch = fixture.request("restart-one", 0, RuntimeChange::Scale { replicas: 2 });
	// Assert
	assert_eq!(completed.id, first.id);
	assert_eq!(completed.state, OperationState::Succeeded);
	assert_eq!(
		pending_for_cluster(&mut resumed_connection, fixture.cluster)
			.await
			.unwrap(),
		vec![]
	);
	assert!(matches!(
		accept_operation(&resumed_connection, fixture.user, &mismatch).await,
		Err(AcceptanceError::IdempotencyConflict)
	));

	// Arrange
	let next = fixture.request("restart-two", 1, RuntimeChange::Restart);
	let competitor = fixture.request("scale-two", 1, RuntimeChange::Scale { replicas: 2 });
	// Act
	let (left, right) = tokio::join!(
		accept_operation(&resumed_connection, fixture.user, &next),
		accept_operation(&resumed_connection, fixture.user, &competitor),
	);
	let second = match (left, right) {
		(Ok(operation), Err(AcceptanceError::VersionConflict))
		| (Err(AcceptanceError::VersionConflict), Ok(operation)) => operation,
		_ => panic!("Exactly one mutation must win the Environment version"),
	};
	// Assert
	assert_eq!(second.environment_version, 2);
	assert!(matches!(
		reconcile_result(&resumed_connection, fixture.cluster, &completed).await,
		Err(AcceptanceError::StaleResult)
	));
	assert!(matches!(
		mark_deadline_uncertain(
			&resumed_connection,
			fixture.cluster,
			&second,
			chrono::Utc::now()
		)
		.await,
		Err(AcceptanceError::UnsafeTransition)
	));

	// Act: a lost receipt must not release the serialization gate at deadline.
	let uncertain = mark_deadline_uncertain(
		&resumed_connection,
		fixture.cluster,
		&second,
		chrono::Utc::now() + chrono::Duration::minutes(11),
	)
	.await
	.unwrap();
	let conflicting = fixture.request("restart-three", 2, RuntimeChange::Restart);
	// Assert
	assert_eq!(uncertain.state, OperationState::Uncertain);
	assert!(matches!(
		accept_operation(&resumed_connection, fixture.user, &conflicting).await,
		Err(AcceptanceError::EnvironmentBusy)
	));
	assert_eq!(
		Operation::objects()
			.all()
			.all_with_db(&mut resumed_connection)
			.await
			.unwrap()
			.len(),
		2
	);
	assert_eq!(
		DispatchRecord::objects()
			.all()
			.all_with_db(&mut resumed_connection)
			.await
			.unwrap()
			.len(),
		2
	);
	assert_eq!(
		AuditRecord::objects()
			.all()
			.all_with_db(&mut resumed_connection)
			.await
			.unwrap()
			.len(),
		2
	);
}

#[rstest]
#[tokio::test]
async fn production_grants_current_membership_and_cluster_limits_gate_acceptance() {
	// Arrange
	let fixture = OperationFixture::new("developer", "production").await;
	let mut connection = fixture.lease.handle();
	let request = fixture.request("production-restart", 0, RuntimeChange::Restart);
	// Act
	let denied = accept_operation(&connection, fixture.user, &request).await;
	// Assert
	assert!(matches!(denied, Err(AcceptanceError::Forbidden)));
	assert_eq!(fixture.environment(&mut connection).await.version, 0);
	assert_eq!(
		Operation::objects()
			.all()
			.all_with_db(&mut connection)
			.await
			.unwrap()
			.len(),
		0
	);

	// Arrange
	let grant = EnvironmentGrant::new()
		.organization(fixture.organization)
		.user(fixture.user)
		.environment(fixture.environment)
		.finish();
	EnvironmentGrant::objects()
		.create_with_conn(&mut connection, &grant)
		.await
		.unwrap();
	let above_budget = fixture.request(
		"scale-above-budget",
		0,
		RuntimeChange::Scale { replicas: 4 },
	);
	Cluster::objects()
		.filter(Cluster::field_id().eq(fixture.cluster))
		.update_fields_with_conn(
			&mut connection,
			[Cluster::field_cpu_autoscaling().assign(false)],
		)
		.await
		.unwrap();
	let unsupported = fixture.request(
		"unsupported-autoscaling",
		0,
		RuntimeChange::Autoscale {
			configuration: CpuAutoscaling {
				min_replicas: 1,
				max_replicas: 3,
				target_cpu_percent: 70,
			},
		},
	);
	// Act
	let budget_denied = accept_operation(&connection, fixture.user, &above_budget).await;
	let capability_denied = accept_operation(&connection, fixture.user, &unsupported).await;
	// Assert
	assert!(matches!(budget_denied, Err(AcceptanceError::NotReady)));
	assert!(matches!(capability_denied, Err(AcceptanceError::NotReady)));
	assert_eq!(fixture.environment(&mut connection).await.version, 0);
	assert_eq!(
		DispatchRecord::objects()
			.all()
			.all_with_db(&mut connection)
			.await
			.unwrap()
			.len(),
		0
	);

	// Act
	let accepted = accept_operation(&connection, fixture.user, &request)
		.await
		.unwrap();
	Membership::objects()
		.filter(Membership::field_user_id().eq(fixture.user))
		.update_fields_with_conn(&mut connection, [Membership::field_role().assign("viewer")])
		.await
		.unwrap();
	let after_demotion = accept_operation(&connection, fixture.user, &request).await;
	// Assert
	assert_eq!(accepted.state, OperationState::Queued);
	assert!(matches!(after_demotion, Err(AcceptanceError::Forbidden)));
	assert_eq!(
		Operation::objects()
			.all()
			.all_with_db(&mut connection)
			.await
			.unwrap()
			.len(),
		1
	);
}

#[rstest]
#[tokio::test]
async fn audit_failure_rolls_back_configuration_operation_and_delivery_intent() {
	// Arrange
	let fixture = OperationFixture::new("owner", "staging").await;
	let mut connection = fixture.lease.handle();
	let mut statement = Query::drop_table();
	statement.table("cloud_observability_auditrecord");
	let (sql, values) = PostgresQueryBuilder::new().build_drop_table(&statement);
	assert_eq!(values.len(), 0);
	connection.execute(&sql, vec![]).await.unwrap();
	let request = fixture.request("will-roll-back", 0, RuntimeChange::Scale { replicas: 3 });
	// Act
	let result = accept_operation(&connection, fixture.user, &request).await;
	let stored = fixture.environment(&mut connection).await;
	// Assert
	assert!(matches!(result, Err(AcceptanceError::Storage(_))));
	assert_eq!(stored.version, 0);
	assert_eq!(
		serde_json::from_str::<DesiredRuntime>(&stored.desired_runtime).unwrap(),
		DesiredRuntime::default()
	);
	assert_eq!(
		Operation::objects()
			.all()
			.all_with_db(&mut connection)
			.await
			.unwrap()
			.len(),
		0
	);
	assert_eq!(
		DispatchRecord::objects()
			.all()
			.all_with_db(&mut connection)
			.await
			.unwrap()
			.len(),
		0
	);
}
