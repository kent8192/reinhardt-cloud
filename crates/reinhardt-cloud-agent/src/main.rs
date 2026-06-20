//! Reinhardt Cloud Cluster Agent
//!
//! Per-cluster agent that connects to the control plane via bidirectional
//! gRPC streaming. Handles deployments, health reporting, and log collection.

use std::time::Duration;

use chrono::Utc;
use clap::Parser;
use k8s_openapi::api::apps::v1::Deployment;
use kube::api::{Api, Patch, PatchParams};
use prost_types::Timestamp;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tracing::{error, info, warn};
use uuid::Uuid;

use reinhardt_cloud_proto::cluster_agent::{self as pb, agent_service_client::AgentServiceClient};

/// Reinhardt Cloud Cluster Agent CLI arguments.
#[derive(Parser)]
#[command(
	name = "reinhardt-cloud-agent",
	about = "Reinhardt Cloud Cluster Agent"
)]
struct Args {
	/// Control plane gRPC endpoint.
	#[arg(
		long,
		env = "CONTROL_PLANE_URL",
		default_value = "http://127.0.0.1:50051"
	)]
	control_plane_url: String,

	/// Cluster name this agent belongs to.
	#[arg(long, env = "CLUSTER_NAME")]
	cluster_name: String,

	/// Node name where this agent runs.
	#[arg(long, env = "NODE_NAME", default_value = "unknown")]
	node_name: String,

	/// Heartbeat interval in seconds.
	#[arg(long, default_value = "30")]
	heartbeat_interval: u64,

	/// JWT token for authentication.
	#[arg(long, env = "AUTH_TOKEN")]
	auth_token: String,
}

fn timestamp_now() -> Option<Timestamp> {
	let now = Utc::now();
	Some(Timestamp {
		seconds: now.timestamp(),
		nanos: now.timestamp_subsec_nanos() as i32,
	})
}

#[tokio::main]
async fn main() {
	// Explicitly install rustls CryptoProvider (defense-in-depth, see #314)
	rustls::crypto::ring::default_provider()
		.install_default()
		.ok();

	tracing_subscriber::fmt::init();

	let args = Args::parse();
	let agent_id = Uuid::now_v7();

	info!(
		agent_id = %agent_id,
		cluster = %args.cluster_name,
		node = %args.node_name,
		"Starting Reinhardt Cloud Agent"
	);

	loop {
		match run_agent(&args, agent_id).await {
			Ok(()) => {
				info!("Agent stream ended, reconnecting...");
			}
			Err(e) => {
				error!("Agent error: {e}, reconnecting in 5s...");
			}
		}

		// Reconnection with backoff
		tokio::time::sleep(Duration::from_secs(5)).await;
	}
}

async fn run_agent(args: &Args, agent_id: Uuid) -> Result<(), Box<dyn std::error::Error>> {
	let channel = Channel::from_shared(args.control_plane_url.clone())?
		.connect()
		.await?;

	let auth_header = build_auth_header(&args.auth_token)?;
	#[allow(clippy::result_large_err)] // tonic interceptor signature requires Result<_, Status>
	let mut client =
		AgentServiceClient::with_interceptor(channel, move |mut req: tonic::Request<()>| {
			req.metadata_mut()
				.insert("authorization", auth_header.clone());
			Ok(req)
		});

	// Create the outbound event stream
	let (event_tx, event_rx) = mpsc::channel::<pb::AgentEvent>(64);

	// Send initial connected event
	event_tx
		.send(pb::AgentEvent {
			event: Some(pb::agent_event::Event::Connected(pb::AgentConnected {
				agent_id: agent_id.to_string(),
				cluster_name: args.cluster_name.clone(),
				timestamp: timestamp_now(),
			})),
		})
		.await?;

	// Start the bidirectional stream
	let response = client.agent_stream(ReceiverStream::new(event_rx)).await?;

	let mut command_stream = response.into_inner();

	// Spawn heartbeat sender
	let heartbeat_tx = event_tx.clone();
	let heartbeat_interval = Duration::from_secs(args.heartbeat_interval);
	let hb_agent_id = agent_id;
	tokio::spawn(async move {
		loop {
			tokio::time::sleep(heartbeat_interval).await;
			let event = pb::AgentEvent {
				event: Some(pb::agent_event::Event::Heartbeat(pb::AgentHeartbeat {
					agent_id: hb_agent_id.to_string(),
					timestamp: timestamp_now(),
				})),
			};
			if heartbeat_tx.send(event).await.is_err() {
				break;
			}
		}
	});

	// Process incoming commands
	while let Some(result) = tokio_stream::StreamExt::next(&mut command_stream).await {
		match result {
			Ok(command) => {
				handle_command(&command, &event_tx).await;
			}
			Err(e) => {
				warn!("Command stream error: {e}");
				break;
			}
		}
	}

	Ok(())
}

fn build_auth_header(token: &str) -> Result<MetadataValue<tonic::metadata::Ascii>, String> {
	let token = token.trim();
	if token.is_empty() {
		return Err("AUTH_TOKEN must not be empty".to_string());
	}

	MetadataValue::try_from(format!("Bearer {token}"))
		.map_err(|e| format!("AUTH_TOKEN cannot be encoded as gRPC metadata: {e}"))
}

async fn handle_command(command: &pb::AgentCommand, event_tx: &mpsc::Sender<pb::AgentEvent>) {
	match &command.command {
		Some(pb::agent_command::Command::Deploy(cmd)) => {
			info!(
				app = %cmd.project_name,
				image = %cmd.image,
				replicas = cmd.replicas,
				"Received deploy command"
			);
			warn!(
				app = %cmd.project_name,
				"Rejected legacy deploy command; apply Project resources through the operator-controlled path"
			);
			let success = false;
			let message =
				"Legacy Deploy commands are disabled; use ApplyProject for operator-validated deployments"
					.to_string();
			if let Err(e) = event_tx
				.send(pb::AgentEvent {
					event: Some(pb::agent_event::Event::DeployStatus(
						pb::AgentDeployStatus {
							project_name: cmd.project_name.clone(),
							success,
							message,
							timestamp: timestamp_now(),
						},
					)),
				})
				.await
			{
				error!("Failed to send deploy status event: {e}");
			}
		}
		Some(pb::agent_command::Command::Rollback(cmd)) => {
			info!(
				app = %cmd.project_name,
				revision = cmd.revision,
				"Received rollback command"
			);
			let (success, message) = match execute_rollback(&cmd.project_name, cmd.revision).await {
				Ok(()) => {
					info!(app = %cmd.project_name, "Rollback applied successfully");
					(true, "Rollback applied".to_string())
				}
				Err(e) => {
					error!(app = %cmd.project_name, error = %e, "Rollback failed");
					(false, format!("Rollback failed: {e}"))
				}
			};
			if let Err(e) = event_tx
				.send(pb::AgentEvent {
					event: Some(pb::agent_event::Event::CommandStatus(
						pb::AgentCommandStatus {
							project_name: cmd.project_name.clone(),
							command_type: "rollback".to_string(),
							success,
							message,
							timestamp: timestamp_now(),
						},
					)),
				})
				.await
			{
				error!("Failed to send rollback status event: {e}");
			}
		}
		Some(pb::agent_command::Command::Scale(cmd)) => {
			info!(
				app = %cmd.project_name,
				replicas = cmd.replicas,
				"Received scale command"
			);
			let (success, message) = match execute_scale(&cmd.project_name, cmd.replicas).await {
				Ok(()) => {
					info!(app = %cmd.project_name, replicas = cmd.replicas, "Scale applied successfully");
					(true, "Scale applied".to_string())
				}
				Err(e) => {
					error!(app = %cmd.project_name, error = %e, "Scale failed");
					(false, format!("Scale failed: {e}"))
				}
			};
			if let Err(e) = event_tx
				.send(pb::AgentEvent {
					event: Some(pb::agent_event::Event::CommandStatus(
						pb::AgentCommandStatus {
							project_name: cmd.project_name.clone(),
							command_type: "scale".to_string(),
							success,
							message,
							timestamp: timestamp_now(),
						},
					)),
				})
				.await
			{
				error!("Failed to send scale status event: {e}");
			}
		}
		Some(pb::agent_command::Command::Restart(cmd)) => {
			info!(app = %cmd.project_name, "Received restart command");
			let (success, message) = match execute_restart(&cmd.project_name).await {
				Ok(()) => {
					info!(app = %cmd.project_name, "Restart applied successfully");
					(true, "Restart applied".to_string())
				}
				Err(e) => {
					error!(app = %cmd.project_name, error = %e, "Restart failed");
					(false, format!("Restart failed: {e}"))
				}
			};
			if let Err(e) = event_tx
				.send(pb::AgentEvent {
					event: Some(pb::agent_event::Event::CommandStatus(
						pb::AgentCommandStatus {
							project_name: cmd.project_name.clone(),
							command_type: "restart".to_string(),
							success,
							message,
							timestamp: timestamp_now(),
						},
					)),
				})
				.await
			{
				error!("Failed to send restart status event: {e}");
			}
		}
		Some(pb::agent_command::Command::ApplyProject(cmd)) => {
			info!(app = %cmd.project_name, "Received Project apply command");
			let (success, message) = match execute_apply_project(&cmd.yaml).await {
				Ok(()) => (true, "Project applied".to_string()),
				Err(e) => {
					error!(app = %cmd.project_name, error = %e, "Project apply failed");
					(false, format!("Project apply failed: {e}"))
				}
			};
			if let Err(e) = event_tx
				.send(pb::AgentEvent {
					event: Some(pb::agent_event::Event::CommandStatus(
						pb::AgentCommandStatus {
							project_name: cmd.project_name.clone(),
							command_type: "apply_project".to_string(),
							success,
							message,
							timestamp: timestamp_now(),
						},
					)),
				})
				.await
			{
				error!("Failed to send Project apply status event: {e}");
			}
		}
		Some(pb::agent_command::Command::ApplyGitCredentialsSecret(cmd)) => {
			info!(app = %cmd.project_name, namespace = %cmd.namespace, secret = %cmd.secret_name, "Received git credentials Secret apply command");
			let (success, message) = match execute_apply_git_credentials_secret(
				&cmd.namespace,
				&cmd.secret_name,
				&cmd.git_token,
			)
			.await
			{
				Ok(()) => (true, "Git credentials Secret applied".to_string()),
				Err(e) => {
					error!(app = %cmd.project_name, error = %e, "Git credentials Secret apply failed");
					(false, format!("Git credentials Secret apply failed: {e}"))
				}
			};
			if let Err(e) = event_tx
				.send(pb::AgentEvent {
					event: Some(pb::agent_event::Event::CommandStatus(
						pb::AgentCommandStatus {
							project_name: cmd.project_name.clone(),
							command_type: "apply_git_credentials_secret".to_string(),
							success,
							message,
							timestamp: timestamp_now(),
						},
					)),
				})
				.await
			{
				error!("Failed to send git credentials Secret apply status event: {e}");
			}
		}
		None => {
			warn!("Received empty command");
		}
	}
}

async fn execute_apply_project(yaml: &str) -> Result<(), String> {
	let app =
		reinhardt_cloud_k8s::resources::parse_project_yaml(yaml).map_err(|e| e.to_string())?;
	let namespace = app.metadata.namespace.as_deref().unwrap_or("default");
	let client = reinhardt_cloud_k8s::KubeClient::from_kubeconfig(namespace)
		.await
		.map_err(|e| e.to_string())?;
	reinhardt_cloud_k8s::resources::server_side_apply_project_yaml(&client, yaml)
		.await
		.map(|_| ())
		.map_err(|e| e.to_string())
}

async fn execute_apply_git_credentials_secret(
	namespace: &str,
	secret_name: &str,
	git_token: &str,
) -> Result<(), String> {
	let client = reinhardt_cloud_k8s::KubeClient::from_kubeconfig(namespace)
		.await
		.map_err(|e| e.to_string())?;
	reinhardt_cloud_k8s::resources::server_side_apply_git_credentials_secret(
		&client,
		namespace,
		secret_name,
		git_token,
	)
	.await
	.map(|_| ())
	.map_err(|e| e.to_string())
}

/// Rollback a Kubernetes Deployment to a previous revision.
///
/// Reads the target ReplicaSet's pod template spec and patches it onto
/// the Deployment, triggering a rollout to the desired revision. This
/// mirrors the behaviour of `kubectl rollout undo --to-revision`.
async fn execute_rollback(project_name: &str, revision: u32) -> Result<(), kube::Error> {
	use k8s_openapi::api::apps::v1::ReplicaSet;

	let client = kube::Client::try_default().await?;
	let deployments: Api<Deployment> = Api::default_namespaced(client.clone());
	let replica_sets: Api<ReplicaSet> = Api::default_namespaced(client);

	// Find the ReplicaSet with the target revision annotation
	let rs_list = replica_sets
		.list(
			&kube::api::ListParams::default()
				.labels(&format!("app.kubernetes.io/name={project_name}")),
		)
		.await?;

	let target_rs = rs_list
		.items
		.iter()
		.find(|rs| {
			rs.metadata
				.annotations
				.as_ref()
				.and_then(|a| a.get("deployment.kubernetes.io/revision"))
				.is_some_and(|v| v == &revision.to_string())
		})
		.ok_or_else(|| {
			kube::Error::Api(
				kube::core::Status::failure(
					&format!("ReplicaSet with revision {revision} not found"),
					"NotFound",
				)
				.boxed(),
			)
		})?;

	// Extract the pod template from the target ReplicaSet and apply it
	// to the Deployment via strategic merge patch.
	let template = target_rs
		.spec
		.as_ref()
		.and_then(|s| s.template.as_ref())
		.ok_or_else(|| {
			kube::Error::Api(
				kube::core::Status::failure(
					&format!("ReplicaSet revision {revision} has no pod template in its spec"),
					"InvalidSpec",
				)
				.boxed(),
			)
		})?;

	let patch = serde_json::json!({
		"spec": {
			"template": template
		}
	});
	deployments
		.patch(
			project_name,
			&PatchParams::default(),
			&Patch::Strategic(patch),
		)
		.await?;

	Ok(())
}

/// Scale a Kubernetes Deployment to the specified number of replicas.
///
/// Patches only the `spec.replicas` field, leaving every other field
/// unchanged. The cluster scheduler then reconciles the actual pod count.
///
/// Returns an error if `replicas` exceeds `i32::MAX` because Kubernetes
/// `spec.replicas` is an `int32` field.
async fn execute_scale(project_name: &str, replicas: u32) -> Result<(), kube::Error> {
	// Kubernetes spec.replicas is int32; reject values that would overflow.
	if replicas > i32::MAX as u32 {
		return Err(kube::Error::Api(
			kube::core::Status::failure(
				&format!(
					"replicas value {replicas} exceeds Kubernetes int32 limit ({})",
					i32::MAX
				),
				"Invalid",
			)
			.boxed(),
		));
	}

	let client = kube::Client::try_default().await?;
	let deployments: Api<Deployment> = Api::default_namespaced(client);

	let patch = serde_json::json!({
		"spec": { "replicas": replicas }
	});
	deployments
		.patch(
			project_name,
			&PatchParams::default(),
			&Patch::Strategic(patch),
		)
		.await?;

	Ok(())
}

/// Perform a rolling restart of a Kubernetes Deployment.
///
/// Sets an annotation on the pod template with the current timestamp, which
/// forces the deployment controller to create new pods — the same mechanism
/// that `kubectl rollout restart` uses.
async fn execute_restart(project_name: &str) -> Result<(), kube::Error> {
	let client = kube::Client::try_default().await?;
	let deployments: Api<Deployment> = Api::default_namespaced(client);

	let patch = serde_json::json!({
		"spec": {
			"template": {
				"metadata": {
					"annotations": {
						"kubectl.kubernetes.io/restartedAt": Utc::now().to_rfc3339()
					}
				}
			}
		}
	});
	deployments
		.patch(
			project_name,
			&PatchParams::default(),
			&Patch::Strategic(patch),
		)
		.await?;

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn build_auth_header_rejects_empty_token() {
		// Arrange
		let token = "   ";

		// Act
		let result = build_auth_header(token);

		// Assert
		assert_eq!(result.unwrap_err(), "AUTH_TOKEN must not be empty");
	}

	#[rstest]
	fn build_auth_header_formats_bearer_token() {
		// Arrange
		let token = "agent.jwt.token";

		// Act
		let result = build_auth_header(token).expect("token should produce metadata");

		// Assert
		assert_eq!(result.to_str().unwrap(), "Bearer agent.jwt.token");
	}

	#[rstest]
	#[tokio::test]
	async fn handle_command_rejects_legacy_deploy_command() {
		// Arrange
		let command = pb::AgentCommand {
			command: Some(pb::agent_command::Command::Deploy(pb::DeployCommand {
				project_name: "web".to_string(),
				image: "attacker.example/payload:latest".to_string(),
				replicas: 2,
			})),
		};
		let (event_tx, mut event_rx) = mpsc::channel::<pb::AgentEvent>(1);

		// Act
		handle_command(&command, &event_tx).await;
		let event = event_rx
			.recv()
			.await
			.expect("deploy rejection status should be sent");

		// Assert
		let Some(pb::agent_event::Event::DeployStatus(status)) = event.event else {
			panic!("expected deploy status event");
		};
		assert_eq!(status.project_name, "web");
		assert!(!status.success);
		assert_eq!(
			status.message,
			"Legacy Deploy commands are disabled; use ApplyProject for operator-validated deployments"
		);
	}
}
