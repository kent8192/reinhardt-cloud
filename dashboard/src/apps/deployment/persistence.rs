//! Transactional operation acceptance and durable, correlated dispatch.

use reinhardt::db::orm::{DatabaseConnection, Model, transaction::AtomicTransaction};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::models::{DispatchRecord, Operation};
use super::services::{
	DispatchEnvelope, OperationProgress, OperationRequest, OperationState, RuntimeChange,
};
use crate::apps::cluster::models::{Cluster, EnvironmentAllocation};
use crate::apps::cluster::services::ClusterCapabilities;
use crate::apps::identity::models::UserAccount;
use crate::apps::observability::models::AuditRecord;
use crate::apps::organization::models::{EnvironmentGrant, Membership};
use crate::apps::organization::services::{
	Action, AuthorizationService, EnvironmentGrant as GrantFacts, Membership as MembershipFacts,
	Role,
};
use crate::apps::project::models::{Environment, Project};
use crate::apps::project::services::{DesiredRuntime, EnvironmentKind};

#[derive(Debug, thiserror::Error)]
pub enum AcceptanceError {
	#[error("Operation access is forbidden")]
	Forbidden,
	#[error("The operation request is invalid")]
	InvalidRequest,
	#[error("An idempotency key was already used for another request")]
	IdempotencyConflict,
	#[error("The Environment version changed")]
	VersionConflict,
	#[error("An unfinished operation blocks this Environment")]
	EnvironmentBusy,
	#[error("The runtime allocation cannot accept this operation")]
	NotReady,
	#[error("The result does not match the current operation")]
	StaleResult,
	#[error("This operation transition is unsafe")]
	UnsafeTransition,
	#[error("Operation storage contains an unsupported state")]
	InvalidStoredState,
	#[error("Operation storage is unavailable")]
	Storage(#[from] reinhardt::core::exception::Error),
	#[error("Operation storage is unavailable")]
	Transaction(#[from] reinhardt::db::backends::error::DatabaseError),
}

fn progress(operation: &Operation) -> Result<OperationProgress, AcceptanceError> {
	Ok(OperationProgress {
		id: operation.id,
		environment_id: operation.environment_id(),
		environment_version: operation.environment_version,
		state: OperationState::from_storage(&operation.state)
			.ok_or(AcceptanceError::InvalidStoredState)?,
	})
}

async fn lock_environment(
	transaction: &mut AtomicTransaction,
	organization: Uuid,
	environment: Uuid,
) -> Result<Environment, AcceptanceError> {
	let environments = Environment::objects()
		.filter(Environment::field_id().eq(environment))
		.filter(Environment::field_organization_id().eq(organization))
		.select_for_update()
		.all_with_executor(transaction)
		.await?;
	let environment = environments
		.into_iter()
		.next()
		.ok_or(AcceptanceError::Forbidden)?;
	let projects = Project::objects()
		.filter(Project::field_id().eq(environment.project_id()))
		.filter(Project::field_organization_id().eq(organization))
		.all_with_db(transaction)
		.await?;
	if projects.is_empty() {
		return Err(AcceptanceError::Forbidden);
	}
	Ok(environment)
}

/// Account and membership locks establish the same authorization boundary as acceptance.
async fn lock_membership(
	transaction: &mut AtomicTransaction,
	user: Uuid,
	organization: Uuid,
) -> Result<MembershipFacts, AcceptanceError> {
	let users = UserAccount::objects()
		.filter(UserAccount::field_id().eq(user))
		.filter(UserAccount::field_active().eq(true))
		.select_for_update()
		.all_with_executor(transaction)
		.await?;
	if users.is_empty() {
		return Err(AcceptanceError::Forbidden);
	}
	let members = Membership::objects()
		.filter(Membership::field_user_id().eq(user))
		.filter(Membership::field_organization_id().eq(organization))
		.select_for_update()
		.all_with_executor(transaction)
		.await?;
	let member = members.first().ok_or(AcceptanceError::Forbidden)?;
	Ok(MembershipFacts {
		user_id: user,
		organization_id: organization,
		role: Role::from_storage(&member.role).ok_or(AcceptanceError::Forbidden)?,
	})
}

async fn authorize_change(
	transaction: &mut AtomicTransaction,
	membership: MembershipFacts,
	environment: &Environment,
) -> Result<(), AcceptanceError> {
	let kind = EnvironmentKind::from_storage(&environment.kind)
		.ok_or(AcceptanceError::InvalidStoredState)?;
	let grants = EnvironmentGrant::objects()
		.filter(EnvironmentGrant::field_user_id().eq(membership.user_id))
		.filter(EnvironmentGrant::field_organization_id().eq(membership.organization_id))
		.filter(EnvironmentGrant::field_environment_id().eq(environment.id))
		.select_for_update()
		.all_with_executor(transaction)
		.await?;
	let grants = grants
		.into_iter()
		.map(|grant| GrantFacts {
			user_id: grant.user_id(),
			organization_id: grant.organization_id(),
			environment_id: grant.environment_id(),
		})
		.collect::<Vec<_>>();
	AuthorizationService::new(&[membership], &grants)
		.authorize(
			membership.user_id,
			membership.organization_id,
			Some((environment.id, kind)),
			Action::ChangeApplication,
		)
		.map_err(|_| AcceptanceError::Forbidden)
}

async fn allocation(
	transaction: &mut AtomicTransaction,
	environment: Uuid,
) -> Result<(Uuid, ClusterCapabilities, u32), AcceptanceError> {
	let allocations = EnvironmentAllocation::objects()
		.filter(EnvironmentAllocation::field_environment_id().eq(environment))
		.select_for_update()
		.all_with_executor(transaction)
		.await?;
	let allocation = allocations.first().ok_or(AcceptanceError::NotReady)?;
	let clusters = Cluster::objects()
		.filter(Cluster::field_id().eq(allocation.cluster_id()))
		.filter(Cluster::field_registered().eq(true))
		.select_for_update()
		.all_with_executor(transaction)
		.await?;
	let cluster = clusters.first().ok_or(AcceptanceError::NotReady)?;
	let capabilities = ClusterCapabilities {
		rootless_builds: cluster.rootless_builds,
		ingress: cluster.ingress,
		dns: cluster.dns,
		namespace_issuer: cluster.namespace_issuer,
		cpu_autoscaling: cluster.cpu_autoscaling,
	};
	capabilities
		.validate_registration()
		.map_err(|_| AcceptanceError::NotReady)?;
	let limit = u32::try_from(allocation.replica_limit)
		.ok()
		.filter(|limit| *limit > 0)
		.ok_or(AcceptanceError::NotReady)?;
	Ok((cluster.id, capabilities, limit))
}

fn apply_change(
	desired: &mut DesiredRuntime,
	change: &RuntimeChange,
	operation: Uuid,
	capabilities: ClusterCapabilities,
	limit: u32,
) -> Result<(), AcceptanceError> {
	match change {
		RuntimeChange::Restart => desired.restart_nonce = Some(operation),
		RuntimeChange::Scale { replicas } => {
			desired.replicas = *replicas;
			desired.autoscaling = None;
		}
		RuntimeChange::Autoscale { configuration } => {
			if configuration.min_replicas == 0
				|| configuration.min_replicas > configuration.max_replicas
				|| !(1..=100).contains(&configuration.target_cpu_percent)
			{
				return Err(AcceptanceError::InvalidRequest);
			}
			desired.replicas = desired
				.replicas
				.clamp(configuration.min_replicas, configuration.max_replicas);
			desired.autoscaling = Some(configuration.clone());
		}
	}
	let admitted_replicas = desired
		.autoscaling
		.as_ref()
		.map_or(desired.replicas, |configuration| configuration.max_replicas);
	capabilities
		.admit_scale(admitted_replicas, limit, desired.autoscaling.is_some())
		.map_err(|_| AcceptanceError::NotReady)
}

/// Desired state, operation identity, delivery intent, and audit either all commit or all roll back.
pub async fn accept_operation(
	connection: &DatabaseConnection,
	user: Uuid,
	request: &OperationRequest,
) -> Result<OperationProgress, AcceptanceError> {
	if request.expected_version < 0
		|| request.idempotency_key.is_empty()
		|| request.idempotency_key.len() > 200
		|| !request
			.idempotency_key
			.bytes()
			.all(|byte| byte.is_ascii_graphic())
	{
		return Err(AcceptanceError::InvalidRequest);
	}
	let fingerprint = hex::encode(Sha256::digest(
		serde_json::to_vec(&(user, request)).map_err(|_| AcceptanceError::InvalidRequest)?,
	));
	connection
		.atomic(async |transaction| {
			let membership = lock_membership(transaction, user, request.organization_id).await?;
			let environment =
				lock_environment(transaction, request.organization_id, request.environment_id)
					.await?;
			authorize_change(transaction, membership, &environment).await?;
			let existing = Operation::objects()
				.filter(Operation::field_organization_id().eq(request.organization_id))
				.filter(Operation::field_environment_id().eq(environment.id))
				.filter(Operation::field_idempotency_key().eq(&request.idempotency_key))
				.all_with_db(transaction)
				.await?;
			if let Some(operation) = existing.first() {
				if operation.request_fingerprint != fingerprint {
					return Err(AcceptanceError::IdempotencyConflict);
				}
				return progress(operation);
			}
			if request.expected_version != environment.version {
				return Err(AcceptanceError::VersionConflict);
			}
			let previous = Operation::objects()
				.filter(Operation::field_environment_id().eq(environment.id))
				.all_with_db(transaction)
				.await?;
			for operation in previous {
				if progress(&operation)?.blocks_environment() {
					return Err(AcceptanceError::EnvironmentBusy);
				}
			}
			let (cluster, capabilities, limit) = allocation(transaction, environment.id).await?;
			let version = environment
				.version
				.checked_add(1)
				.ok_or(AcceptanceError::VersionConflict)?;
			let mut desired: DesiredRuntime = serde_json::from_str(&environment.desired_runtime)
				.map_err(|_| AcceptanceError::InvalidStoredState)?;
			let operation = Operation::new()
				.organization(request.organization_id)
				.environment(environment.id)
				.environment_version(version)
				.idempotency_key(&request.idempotency_key)
				.request_fingerprint(&fingerprint)
				.state("queued")
				.kind(request.change.storage_name())
				.finish();
			apply_change(
				&mut desired,
				&request.change,
				operation.id,
				capabilities,
				limit,
			)?;
			let serialized =
				serde_json::to_string(&desired).map_err(|_| AcceptanceError::InvalidStoredState)?;
			Environment::objects()
				.filter(Environment::field_id().eq(environment.id))
				.update_fields_with_conn(
					transaction,
					[
						Environment::field_version().assign(version),
						Environment::field_desired_runtime().assign(serialized),
					],
				)
				.await?;
			let operation = Operation::objects()
				.create_with_conn(transaction, &operation)
				.await?;
			let envelope = DispatchEnvelope {
				schema_version: 1,
				operation_id: operation.id,
				organization_id: request.organization_id,
				environment_id: environment.id,
				environment_version: version,
				cluster_id: cluster,
				change: request.change.clone(),
				desired_runtime: desired,
			};
			let dispatch = DispatchRecord::new()
				.operation(operation.id)
				.environment(environment.id)
				.cluster(cluster)
				.environment_version(version)
				.payload(
					serde_json::to_string(&envelope)
						.map_err(|_| AcceptanceError::InvalidStoredState)?,
				)
				.delivered(false)
				.finish();
			DispatchRecord::objects()
				.create_with_conn(transaction, &dispatch)
				.await?;
			let audit = AuditRecord::new()
				.organization_id(request.organization_id)
				.actor_id(user)
				.target_id(operation.id)
				.action(format!(
					"operation.accepted.{}",
					request.change.storage_name()
				))
				.finish();
			AuditRecord::objects()
				.create_with_conn(transaction, &audit)
				.await?;
			progress(&operation)
		})
		.await
}

/// Machine transports must authenticate the cluster before calling this service.
pub async fn pending_for_cluster(
	connection: &mut DatabaseConnection,
	cluster: Uuid,
) -> Result<Vec<DispatchEnvelope>, AcceptanceError> {
	let records = DispatchRecord::objects()
		.filter(DispatchRecord::field_cluster_id().eq(cluster))
		.filter(DispatchRecord::field_delivered().eq(false))
		.order_by(&["id"])
		.limit(100)
		.all_with_db(connection)
		.await?;
	let mut envelopes = Vec::new();
	for record in records {
		let envelope: DispatchEnvelope = serde_json::from_str(&record.payload)
			.map_err(|_| AcceptanceError::InvalidStoredState)?;
		if envelope.schema_version != 1
			|| envelope.cluster_id != cluster
			|| envelope.operation_id != record.operation_id()
			|| envelope.environment_id != record.environment_id()
			|| envelope.environment_version != record.environment_version
		{
			return Err(AcceptanceError::InvalidStoredState);
		}
		envelopes.push(envelope);
	}
	Ok(envelopes)
}

async fn correlated_operation(
	transaction: &mut AtomicTransaction,
	cluster: Uuid,
	result: &OperationProgress,
) -> Result<(Operation, DispatchRecord), AcceptanceError> {
	let operations = Operation::objects()
		.filter(Operation::field_id().eq(result.id))
		.filter(Operation::field_environment_id().eq(result.environment_id))
		.filter(Operation::field_environment_version().eq(result.environment_version))
		.all_with_db(transaction)
		.await?;
	let operation = operations
		.into_iter()
		.next()
		.ok_or(AcceptanceError::StaleResult)?;
	let environment = lock_environment(
		transaction,
		operation.organization_id(),
		result.environment_id,
	)
	.await?;
	if environment.version != result.environment_version {
		return Err(AcceptanceError::StaleResult);
	}
	let records = DispatchRecord::objects()
		.filter(DispatchRecord::field_operation_id().eq(result.id))
		.select_for_update()
		.all_with_executor(transaction)
		.await?;
	let record = records
		.into_iter()
		.next()
		.ok_or(AcceptanceError::StaleResult)?;
	let envelope: DispatchEnvelope =
		serde_json::from_str(&record.payload).map_err(|_| AcceptanceError::InvalidStoredState)?;
	if envelope.schema_version != 1
		|| envelope.operation_id != result.id
		|| envelope.environment_id != result.environment_id
		|| envelope.environment_version != result.environment_version
		|| envelope.cluster_id != cluster
		|| record.cluster_id() != cluster
		|| record.environment_id() != result.environment_id
		|| record.environment_version != result.environment_version
	{
		return Err(AcceptanceError::StaleResult);
	}
	// Re-read after acquiring the Environment lock so concurrent results cannot
	// both transition from an earlier operation state.
	let operations = Operation::objects()
		.filter(Operation::field_id().eq(result.id))
		.all_with_db(transaction)
		.await?;
	Ok((
		operations
			.into_iter()
			.next()
			.ok_or(AcceptanceError::StaleResult)?,
		record,
	))
}

pub async fn record_receipt(
	connection: &DatabaseConnection,
	cluster: Uuid,
	operation: &OperationProgress,
) -> Result<(), AcceptanceError> {
	connection
		.atomic(async |transaction| {
			let (stored, record) = correlated_operation(transaction, cluster, operation).await?;
			if record.delivered {
				return Ok(());
			}
			if !progress(&stored)?.blocks_environment() {
				return Err(AcceptanceError::UnsafeTransition);
			}
			DispatchRecord::objects()
				.filter(DispatchRecord::field_id().eq(record.id))
				.update_fields_with_conn(
					transaction,
					[DispatchRecord::field_delivered().assign(true)],
				)
				.await?;
			Ok(())
		})
		.await
}

pub async fn reconcile_result(
	connection: &DatabaseConnection,
	cluster: Uuid,
	result: &OperationProgress,
) -> Result<OperationProgress, AcceptanceError> {
	connection
		.atomic(async |transaction| {
			let (operation, record) = correlated_operation(transaction, cluster, result).await?;
			if !record.delivered {
				return Err(AcceptanceError::UnsafeTransition);
			}
			let mut current = progress(&operation)?;
			current
				.reconcile(result.id, result.environment_version, result.state)
				.map_err(|_| AcceptanceError::UnsafeTransition)?;
			Operation::objects()
				.filter(Operation::field_id().eq(result.id))
				.update_fields_with_conn(
					transaction,
					[Operation::field_state().assign(current.state.storage_name())],
				)
				.await?;
			Ok(current)
		})
		.await
}

/// Runtime controls have a ten-minute deadline. Missing receipt is also uncertain:
/// the Agent may have executed a request whose acknowledgement was lost.
pub async fn mark_deadline_uncertain(
	connection: &DatabaseConnection,
	cluster: Uuid,
	operation: &OperationProgress,
	now: chrono::DateTime<chrono::Utc>,
) -> Result<OperationProgress, AcceptanceError> {
	connection
		.atomic(async |transaction| {
			let (stored, _) = correlated_operation(transaction, cluster, operation).await?;
			if now < stored.created_at + chrono::Duration::minutes(10) {
				return Err(AcceptanceError::UnsafeTransition);
			}
			let mut current = progress(&stored)?;
			current
				.deadline_exceeded()
				.map_err(|_| AcceptanceError::UnsafeTransition)?;
			Operation::objects()
				.filter(Operation::field_id().eq(current.id))
				.update_fields_with_conn(
					transaction,
					[Operation::field_state().assign("uncertain")],
				)
				.await?;
			Ok(current)
		})
		.await
}
