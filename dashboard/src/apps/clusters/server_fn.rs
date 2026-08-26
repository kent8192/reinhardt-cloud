//! Cluster server functions for the WASM dashboard.
//!
//! Organization-scoped operations resolve RBAC permissions before loading
//! cluster records so read-only members cannot perform mutations.

use reinhardt::pages::ClientForm;
use reinhardt::pages::server_fn::{ServerFnError, server_fn};
use serde::{Deserialize, Serialize};

use crate::apps::clusters::model_form::{ClusterCreateFields, ClusterCreateFormData};

#[cfg(native)]
use reinhardt::core::exception::Error as AppError;
#[cfg(native)]
use reinhardt::core::validators::{UrlValidator, Validator};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClusterInfo {
	pub id: i64,
	pub name: String,
	pub api_url: String,
	pub is_active: bool,
	pub token_last_rotated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClusterTokenInfo {
	pub cluster: ClusterInfo,
	pub auth_token: String,
}

/// Browser payload for updating a cluster in the current organization.
#[reinhardt::dto]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ClientForm)]
#[client_form(
	server_fn = crate::apps::clusters::server_fn::update_cluster_for_current_org,
	validate
)]
pub struct UpdateClusterFormRequest {
	#[validate(length(min = 1))]
	pub cluster_id: String,
	#[validate(length(min = 1, max = 63))]
	pub name: String,
	#[validate(url, length(max = 2048))]
	pub api_url: String,
	pub is_active: bool,
}

#[cfg(native)]
async fn current_org_id_for_action(
	user: &crate::apps::auth::models::User,
	action: crate::apps::organizations::permissions::Action,
) -> Result<i64, ServerFnError> {
	crate::apps::organizations::permissions::require_permission(user.id, action)
		.await
		.map_err(|e| ServerFnError::application(e.to_string()))
}

#[cfg(native)]
fn cluster_info(cluster: crate::apps::clusters::models::Cluster) -> ClusterInfo {
	ClusterInfo {
		id: cluster.id.unwrap_or_default(),
		name: cluster.name,
		api_url: cluster.api_url,
		is_active: cluster.is_active,
		token_last_rotated_at: cluster.token_last_rotated_at.map(|ts| ts.to_rfc3339()),
	}
}

#[cfg(native)]
fn cluster_id_from_pk(id: Option<i64>) -> Result<uuid::Uuid, AppError> {
	let pk = id.ok_or_else(|| {
		AppError::Internal("Cluster row missing primary key after insert".to_string())
	})?;
	let mut bytes = [0u8; 16];
	bytes[..8].copy_from_slice(b"RHCL-CID");
	bytes[8..].copy_from_slice(&pk.to_be_bytes());
	Ok(uuid::Uuid::from_bytes(bytes))
}

#[cfg(native)]
fn validated_cluster_create_payload(
	payload: &ClusterCreateFormData<ClusterCreateFields>,
) -> Result<(String, String), ServerFnError> {
	let name = payload
		.name()
		.map_or_else(String::new, |value| value.trim().to_string());
	let api_url = payload
		.api_url()
		.map_or_else(String::new, |value| value.trim().to_string());
	let mut errors = Vec::new();

	if name.is_empty() {
		errors.push(("name", "This field is required"));
	} else if name.len() > 63 {
		errors.push(("name", "Must be 63 characters or fewer"));
	}
	if api_url.is_empty() {
		errors.push(("api_url", "This field is required"));
	} else if api_url.len() > 2048 {
		errors.push(("api_url", "Must be 2048 characters or fewer"));
	} else if UrlValidator::new().validate(api_url.as_str()).is_err() {
		errors.push(("api_url", "Enter a valid HTTP or HTTPS URL"));
	}

	if errors.is_empty() {
		Ok((name, api_url))
	} else {
		Err(ServerFnError::validation(errors))
	}
}

#[server_fn]
pub async fn list_clusters_for_current_org(
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<Vec<ClusterInfo>, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::clusters::models::Cluster;

	let organization_id = current_org_id_for_action(
		&user,
		crate::apps::organizations::permissions::Action::ClusterRead,
	)
	.await?;
	let clusters = Cluster::objects()
		.filter(Cluster::field_organization_id().eq(organization_id))
		.order_by(&["id"])
		.all()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to list clusters: {e}")))?;
	Ok(clusters.into_iter().map(cluster_info).collect())
}

#[server_fn(model_form = true)]
pub async fn create_cluster_for_current_org(
	payload: ClusterCreateFormData<ClusterCreateFields>,
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
	#[inject] database: reinhardt::db::orm::DatabaseConnection,
	#[inject] agent_token_service: reinhardt::di::KeyedDepends<
		crate::apps::clusters::services::AgentTokenServiceKey,
		crate::apps::clusters::services::AgentTokenService,
	>,
) -> Result<ClusterTokenInfo, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::clusters::models::Cluster;

	let organization_id = current_org_id_for_action(
		&user,
		crate::apps::organizations::permissions::Action::ClusterCreate,
	)
	.await?;
	let (name, api_url) = validated_cluster_create_payload(&payload)?;

	let manager = Cluster::objects();
	let result: Result<(Cluster, String), AppError> = database
		.atomic(async |transaction| {
			let new_cluster = Cluster::build()
				.organization(organization_id)
				.name(name)
				.api_url(api_url)
				.is_active(true)
				.token_hash(None)
				.token_last_rotated_at(None)
				.finish();
			let mut created = manager.create_with_conn(transaction, &new_cluster).await?;
			let cluster_uuid = cluster_id_from_pk(created.id)?;
			let issued = agent_token_service.issue(cluster_uuid)?;
			created.token_hash = Some(issued.hash);
			created.token_last_rotated_at = Some(chrono::Utc::now());
			let updated = manager.update_with_conn(transaction, &created).await?;
			Ok((updated, issued.plaintext))
		})
		.await;
	let (updated, auth_token) = result.map_err(|error| {
		let message = error.to_string();
		if message.to_lowercase().contains("unique") || message.to_lowercase().contains("duplicate")
		{
			ServerFnError::validation([(
				"name",
				"Cluster name already exists in this organization",
			)])
		} else {
			ServerFnError::application(format!("Failed to create cluster: {message}"))
		}
	})?;
	Ok(ClusterTokenInfo {
		cluster: cluster_info(updated),
		auth_token,
	})
}

#[server_fn]
pub async fn update_cluster_for_current_org(
	request: UpdateClusterFormRequest,
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<ClusterInfo, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::clusters::models::Cluster;

	let organization_id = current_org_id_for_action(
		&user,
		crate::apps::organizations::permissions::Action::ClusterUpdate,
	)
	.await?;
	reinhardt::Validate::validate(&request).map_err(ServerFnError::from)?;
	let cluster_id: i64 = request
		.cluster_id
		.parse()
		.map_err(|_| ServerFnError::validation([("cluster_id", "Select a valid cluster")]))?;
	let name = request.name.trim().to_string();
	let api_url = request.api_url.trim().to_string();
	if name.is_empty() || name.len() > 63 {
		return Err(ServerFnError::validation([(
			"name",
			"Cluster name must be 1-63 characters",
		)]));
	}
	if api_url.is_empty() || api_url.len() > 2048 {
		return Err(ServerFnError::validation([(
			"api_url",
			"API URL must be 1-2048 characters",
		)]));
	}

	let manager = Cluster::objects();
	let mut cluster = manager
		.filter(Cluster::field_organization_id().eq(organization_id))
		.filter(Cluster::field_id().eq(cluster_id))
		.first()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to load cluster: {e}")))?
		.ok_or_else(|| {
			ServerFnError::validation([("cluster_id", "The selected cluster is not available")])
		})?;
	cluster.name = name;
	cluster.api_url = api_url;
	cluster.is_active = request.is_active;
	let updated = manager.update(&cluster).await.map_err(|e| {
		let msg = e.to_string();
		if msg.to_lowercase().contains("unique") || msg.to_lowercase().contains("duplicate") {
			ServerFnError::validation([(
				"name",
				"Cluster name already exists in this organization",
			)])
		} else {
			ServerFnError::application(format!("Failed to update cluster: {msg}"))
		}
	})?;
	Ok(cluster_info(updated))
}

#[server_fn]
pub async fn delete_cluster_for_current_org(
	cluster_id: String,
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<(), ServerFnError> {
	use reinhardt::Model;

	use crate::apps::clusters::models::Cluster;

	let organization_id = current_org_id_for_action(
		&user,
		crate::apps::organizations::permissions::Action::ClusterDelete,
	)
	.await?;
	let cluster_id: i64 = cluster_id
		.parse()
		.map_err(|_| ServerFnError::application("Invalid cluster_id"))?;
	Cluster::objects()
		.filter(Cluster::field_organization_id().eq(organization_id))
		.filter(Cluster::field_id().eq(cluster_id))
		.first()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to load cluster: {e}")))?
		.ok_or_else(|| ServerFnError::server(404, "Cluster not found"))?;
	Cluster::objects().delete(cluster_id).await.map_err(|e| {
		let msg = e.to_string();
		if msg.to_lowercase().contains("foreign key") || msg.contains("RESTRICT") {
			ServerFnError::server(409, "Cannot delete cluster with associated deployments")
		} else {
			ServerFnError::application(format!("Failed to delete cluster: {msg}"))
		}
	})?;
	Ok(())
}

#[server_fn]
pub async fn rotate_cluster_token_for_current_org(
	cluster_id: String,
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
	#[inject] agent_token_service: reinhardt::di::KeyedDepends<
		crate::apps::clusters::services::AgentTokenServiceKey,
		crate::apps::clusters::services::AgentTokenService,
	>,
) -> Result<ClusterTokenInfo, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::clusters::models::Cluster;

	let organization_id = current_org_id_for_action(
		&user,
		crate::apps::organizations::permissions::Action::ClusterUpdate,
	)
	.await?;
	let cluster_id: i64 = cluster_id
		.parse()
		.map_err(|_| ServerFnError::application("Invalid cluster_id"))?;
	let manager = Cluster::objects();
	let mut cluster = manager
		.filter(Cluster::field_organization_id().eq(organization_id))
		.filter(Cluster::field_id().eq(cluster_id))
		.first()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to load cluster: {e}")))?
		.ok_or_else(|| ServerFnError::server(404, "Cluster not found"))?;
	let cluster_uuid = cluster_id_from_pk(cluster.id)
		.map_err(|error| ServerFnError::application(error.to_string()))?;
	let issued = agent_token_service
		.issue(cluster_uuid)
		.map_err(|e| ServerFnError::application(format!("Failed to issue agent token: {e}")))?;
	cluster.token_hash = Some(issued.hash);
	cluster.token_last_rotated_at = Some(chrono::Utc::now());
	let updated = manager
		.update(&cluster)
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to persist agent token: {e}")))?;
	Ok(ClusterTokenInfo {
		cluster: cluster_info(updated),
		auth_token: issued.plaintext,
	})
}

#[cfg(all(test, native))]
mod tests {
	use reinhardt::pages::server_fn::ServerFnErrorKind;
	use rstest::rstest;

	use super::validated_cluster_create_payload;
	use crate::apps::clusters::model_form::{ClusterCreateFields, ClusterCreateFormData};

	#[rstest]
	fn cluster_create_model_form_trims_public_values() {
		// Arrange
		let mut payload = ClusterCreateFormData::<ClusterCreateFields>::empty();
		payload
			.set_name(" production ".to_string())
			.expect("set public name");
		payload
			.set_api_url(" https://kubernetes.example.com:6443 ".to_string())
			.expect("set public API URL");

		// Act
		let values = validated_cluster_create_payload(&payload).expect("validate payload");

		// Assert
		assert_eq!(
			values,
			(
				"production".to_string(),
				"https://kubernetes.example.com:6443".to_string(),
			)
		);
	}

	#[rstest]
	fn cluster_create_model_form_rejects_server_owned_fields_at_decode() {
		// Arrange and Act
		let result = serde_json::from_value::<ClusterCreateFormData<ClusterCreateFields>>(
			serde_json::json!({
				"name": "production",
				"api_url": "https://kubernetes.example.com:6443",
				"organization_id": 42,
			}),
		);
		let error = match result {
			Err(error) => error,
			Ok(_) => panic!("reject a server-managed organization ID during decoding"),
		};

		// Assert
		assert_eq!(
			error.to_string(),
			"unknown field `organization_id`, expected `name` or `api_url`"
		);
	}

	#[rstest]
	fn cluster_create_model_form_reports_structured_field_errors() {
		// Arrange
		let payload = serde_json::from_value::<ClusterCreateFormData<ClusterCreateFields>>(
			serde_json::json!({
				"name": "",
				"api_url": "not-a-url",
			}),
		)
		.expect("deserialize cluster model form payload");

		// Act
		let error = validated_cluster_create_payload(&payload)
			.expect_err("reject invalid generated form values");

		// Assert
		assert_eq!(error.kind(), ServerFnErrorKind::Validation);
		assert_eq!(error.field_errors().len(), 2);
		assert_eq!(error.field_errors()[0].field(), "name");
		assert_eq!(error.field_errors()[0].message(), "This field is required");
		assert_eq!(error.field_errors()[1].field(), "api_url");
		assert_eq!(
			error.field_errors()[1].message(),
			"Enter a valid HTTP or HTTPS URL"
		);
	}
}
