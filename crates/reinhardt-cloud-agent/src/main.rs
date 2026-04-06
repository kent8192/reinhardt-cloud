//! Reinhardt Cloud Cluster Agent
//!
//! Per-cluster agent that connects to the control plane via bidirectional
//! gRPC streaming. Handles deployments, health reporting, and log collection.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::Utc;
use clap::Parser;
use k8s_openapi::api::apps::v1::{Deployment, DeploymentSpec};
use k8s_openapi::api::core::v1::{Container, ContainerPort, PodSpec, PodTemplateSpec};
use k8s_openapi::apimachinery::pkg::apis::meta::v1::{LabelSelector, ObjectMeta};
use kube::api::{Api, Patch, PatchParams};
use prost_types::Timestamp;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
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
	auth_token: Option<String>,
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
	tracing_subscriber::fmt::init();

	let args = Args::parse();
	let agent_id = Uuid::new_v4();

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

	let auth_token = args.auth_token.clone();
	#[allow(clippy::result_large_err)] // tonic interceptor signature requires Result<_, Status>
	let mut client =
		AgentServiceClient::with_interceptor(channel, move |mut req: tonic::Request<()>| {
			if let Some(token) = &auth_token {
				req.metadata_mut()
					.insert("authorization", format!("Bearer {token}").parse().unwrap());
			}
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

async fn handle_command(command: &pb::AgentCommand, event_tx: &mpsc::Sender<pb::AgentEvent>) {
	match &command.command {
		Some(pb::agent_command::Command::Deploy(cmd)) => {
			info!(
				app = %cmd.app_name,
				image = %cmd.image,
				replicas = cmd.replicas,
				"Received deploy command"
			);
			let (success, message) =
				match execute_deploy(&cmd.app_name, &cmd.image, cmd.replicas).await {
					Ok(()) => {
						info!(app = %cmd.app_name, "Deployment applied successfully");
						(true, "Deployment applied".to_string())
					}
					Err(e) => {
						error!(app = %cmd.app_name, error = %e, "Deployment failed");
						(false, format!("Deployment failed: {e}"))
					}
				};
			if let Err(e) = event_tx
				.send(pb::AgentEvent {
					event: Some(pb::agent_event::Event::DeployStatus(
						pb::AgentDeployStatus {
							app_name: cmd.app_name.clone(),
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
				app = %cmd.app_name,
				revision = cmd.revision,
				"Received rollback command"
			);
			let (success, message) = match execute_rollback(&cmd.app_name, cmd.revision).await {
				Ok(()) => {
					info!(app = %cmd.app_name, "Rollback applied successfully");
					(true, "Rollback applied".to_string())
				}
				Err(e) => {
					error!(app = %cmd.app_name, error = %e, "Rollback failed");
					(false, format!("Rollback failed: {e}"))
				}
			};
			if let Err(e) = event_tx
				.send(pb::AgentEvent {
					event: Some(pb::agent_event::Event::CommandStatus(
						pb::AgentCommandStatus {
							app_name: cmd.app_name.clone(),
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
				app = %cmd.app_name,
				replicas = cmd.replicas,
				"Received scale command"
			);
			let (success, message) = match execute_scale(&cmd.app_name, cmd.replicas).await {
				Ok(()) => {
					info!(app = %cmd.app_name, replicas = cmd.replicas, "Scale applied successfully");
					(true, "Scale applied".to_string())
				}
				Err(e) => {
					error!(app = %cmd.app_name, error = %e, "Scale failed");
					(false, format!("Scale failed: {e}"))
				}
			};
			if let Err(e) = event_tx
				.send(pb::AgentEvent {
					event: Some(pb::agent_event::Event::CommandStatus(
						pb::AgentCommandStatus {
							app_name: cmd.app_name.clone(),
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
			info!(app = %cmd.app_name, "Received restart command");
			let (success, message) = match execute_restart(&cmd.app_name).await {
				Ok(()) => {
					info!(app = %cmd.app_name, "Restart applied successfully");
					(true, "Restart applied".to_string())
				}
				Err(e) => {
					error!(app = %cmd.app_name, error = %e, "Restart failed");
					(false, format!("Restart failed: {e}"))
				}
			};
			if let Err(e) = event_tx
				.send(pb::AgentEvent {
					event: Some(pb::agent_event::Event::CommandStatus(
						pb::AgentCommandStatus {
							app_name: cmd.app_name.clone(),
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
		None => {
			warn!("Received empty command");
		}
	}
}

/// Execute a Kubernetes Deployment via server-side apply.
///
/// Constructs a minimal `apps/v1 Deployment` with the given parameters and
/// applies it to the `default` namespace. Uses server-side apply so the
/// operation is idempotent — creating the resource if absent or updating it
/// if it already exists.
async fn execute_deploy(app_name: &str, image: &str, replicas: u32) -> Result<(), kube::Error> {
	let client = kube::Client::try_default().await?;
	let deployments: Api<Deployment> = Api::default_namespaced(client);

	let labels = BTreeMap::from([
		("app.kubernetes.io/name".to_string(), app_name.to_string()),
		(
			"app.kubernetes.io/managed-by".to_string(),
			"reinhardt-cloud".to_string(),
		),
	]);

	let deployment = Deployment {
		metadata: ObjectMeta {
			name: Some(app_name.to_string()),
			labels: Some(labels.clone()),
			..Default::default()
		},
		spec: Some(DeploymentSpec {
			replicas: Some(replicas as i32),
			selector: LabelSelector {
				match_labels: Some(labels.clone()),
				..Default::default()
			},
			template: PodTemplateSpec {
				metadata: Some(ObjectMeta {
					labels: Some(labels),
					..Default::default()
				}),
				spec: Some(PodSpec {
					containers: vec![Container {
						name: app_name.to_string(),
						image: Some(image.to_string()),
						ports: Some(vec![ContainerPort {
							container_port: 8000,
							..Default::default()
						}]),
						..Default::default()
					}],
					..Default::default()
				}),
			},
			..Default::default()
		}),
		..Default::default()
	};

	let params = PatchParams::apply("reinhardt-cloud-agent");
	deployments
		.patch(app_name, &params, &Patch::Apply(&deployment))
		.await?;

	Ok(())
}

/// Rollback a Kubernetes Deployment to a previous revision.
///
/// Reads the target ReplicaSet's pod template spec and patches it onto
/// the Deployment, triggering a rollout to the desired revision. This
/// mirrors the behaviour of `kubectl rollout undo --to-revision`.
async fn execute_rollback(app_name: &str, revision: u32) -> Result<(), kube::Error> {
	use k8s_openapi::api::apps::v1::ReplicaSet;

	let client = kube::Client::try_default().await?;
	let deployments: Api<Deployment> = Api::default_namespaced(client.clone());
	let replica_sets: Api<ReplicaSet> = Api::default_namespaced(client);

	// Find the ReplicaSet with the target revision annotation
	let rs_list = replica_sets
		.list(&kube::api::ListParams::default().labels(&format!(
			"app.kubernetes.io/name={app_name}"
		)))
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
			kube::Error::Api(kube::error::ErrorResponse {
				status: "Failure".to_string(),
				message: format!("ReplicaSet with revision {revision} not found"),
				reason: "NotFound".to_string(),
				code: 404,
			})
		})?;

	// Extract the pod template from the target ReplicaSet and apply it
	// to the Deployment via strategic merge patch.
	let template = target_rs
		.spec
		.as_ref()
		.map(|s| &s.template);

	let patch = serde_json::json!({
		"spec": {
			"template": template
		}
	});
	deployments
		.patch(app_name, &PatchParams::default(), &Patch::Strategic(patch))
		.await?;

	Ok(())
}

/// Scale a Kubernetes Deployment to the specified number of replicas.
///
/// Patches only the `spec.replicas` field, leaving every other field
/// unchanged. The cluster scheduler then reconciles the actual pod count.
async fn execute_scale(app_name: &str, replicas: u32) -> Result<(), kube::Error> {
	let client = kube::Client::try_default().await?;
	let deployments: Api<Deployment> = Api::default_namespaced(client);

	let patch = serde_json::json!({
		"spec": { "replicas": replicas }
	});
	deployments
		.patch(app_name, &PatchParams::default(), &Patch::Strategic(patch))
		.await?;

	Ok(())
}

/// Perform a rolling restart of a Kubernetes Deployment.
///
/// Sets an annotation on the pod template with the current timestamp, which
/// forces the deployment controller to create new pods — the same mechanism
/// that `kubectl rollout restart` uses.
async fn execute_restart(app_name: &str) -> Result<(), kube::Error> {
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
		.patch(app_name, &PatchParams::default(), &Patch::Strategic(patch))
		.await?;

	Ok(())
}
