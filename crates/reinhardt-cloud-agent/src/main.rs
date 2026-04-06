//! Reinhardt Cloud Cluster Agent
//!
//! Per-cluster agent that connects to the control plane via bidirectional
//! gRPC streaming. Handles deployments, health reporting, and log collection.

use std::time::Duration;

use chrono::Utc;
use clap::Parser;
use prost_types::Timestamp;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use tracing::{error, info, warn};
use uuid::Uuid;

use reinhardt_cloud_proto::cluster_agent::{
	self as pb, agent_service_client::AgentServiceClient,
};

/// Reinhardt Cloud Cluster Agent CLI arguments.
#[derive(Parser)]
#[command(name = "reinhardt-cloud-agent", about = "Reinhardt Cloud Cluster Agent")]
struct Args {
	/// Control plane gRPC endpoint.
	#[arg(long, env = "CONTROL_PLANE_URL", default_value = "http://127.0.0.1:50051")]
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

	let mut client = AgentServiceClient::new(channel);

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
	let response = client
		.agent_stream(ReceiverStream::new(event_rx))
		.await?;

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

async fn handle_command(command: &pb::AgentCommand, _event_tx: &mpsc::Sender<pb::AgentEvent>) {
	match &command.command {
		Some(pb::agent_command::Command::Deploy(cmd)) => {
			info!(
				app = %cmd.app_name,
				image = %cmd.image,
				replicas = cmd.replicas,
				"Received deploy command"
			);
			// TODO: Execute deployment via Kubernetes API
		}
		Some(pb::agent_command::Command::Rollback(cmd)) => {
			info!(
				app = %cmd.app_name,
				revision = cmd.revision,
				"Received rollback command"
			);
		}
		Some(pb::agent_command::Command::Scale(cmd)) => {
			info!(
				app = %cmd.app_name,
				replicas = cmd.replicas,
				"Received scale command"
			);
		}
		Some(pb::agent_command::Command::Restart(cmd)) => {
			info!(app = %cmd.app_name, "Received restart command");
		}
		None => {
			warn!("Received empty command");
		}
	}
}
