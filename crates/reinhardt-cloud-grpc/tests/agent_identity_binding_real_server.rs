//! Integration tests proving that `AgentService` binds every agent-reported
//! identity to the cluster the caller authenticated as.
//!
//! The agent `agent_id` carried in a health report or in the `Connected`
//! event is chosen by the peer. A valid agent token only proves membership of
//! one cluster, so these tests run a real tonic server wired like
//! `dashboard/src/config/grpc.rs::start_grpc_server` (`AgentJwtInterceptor` in
//! front of `RegistryBackedAgentService`) and check, through a real client,
//! that an agent of one cluster cannot act on an agent of another cluster.

use std::net::SocketAddr;
use std::sync::Arc;

use chrono::Utc;
use reinhardt_cloud_grpc::agent_claims::create_agent_token;
use reinhardt_cloud_grpc::interceptor::{AgentJwtInterceptor, intercepted};
use reinhardt_cloud_grpc::registry::{AgentRegistry, ClusterRegistration};
use reinhardt_cloud_grpc::services::cluster_agent::{AgentServiceGrpc, RegistryBackedAgentService};
use reinhardt_cloud_proto::cluster_agent as pb;
use reinhardt_cloud_proto::cluster_agent::agent_service_client::AgentServiceClient;
use reinhardt_cloud_proto::cluster_agent::agent_service_server::AgentServiceServer;
use reinhardt_cloud_types::agent::{AgentCommand, AgentInfo};
use rstest::{fixture, rstest};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Channel, Server};
use tonic::{Code, Request};
use uuid::Uuid;

const JWT_SECRET: &[u8] = b"agent-identity-binding-test-secret";

/// The one message every binding failure returns to the client.
const BINDING_DENIED: &str = "Agent is not registered under the authenticated cluster";

/// A running tonic server; the serving task is aborted on drop.
struct TestServer {
	addr: SocketAddr,
	registry: Arc<AgentRegistry>,
	handle: tokio::task::JoinHandle<()>,
}

impl Drop for TestServer {
	fn drop(&mut self) {
		self.handle.abort();
	}
}

impl TestServer {
	async fn client(&self) -> AgentServiceClient<Channel> {
		let endpoint = format!("http://{}", self.addr);
		// The listener is bound already; retry only covers the serving task not having polled yet.
		for _ in 0..40 {
			if let Ok(channel) = Channel::from_shared(endpoint.clone())
				.expect("valid endpoint")
				.connect()
				.await
			{
				return AgentServiceClient::new(channel);
			}
			tokio::time::sleep(std::time::Duration::from_millis(50)).await;
		}
		panic!("could not connect to test gRPC server at {endpoint}");
	}

	/// Register an agent under `cluster_id` the way a completed handshake does.
	///
	/// The returned receiver keeps the registration alive and yields the
	/// commands routed to the agent.
	fn connect_agent(&self, agent_id: Uuid, cluster_id: Uuid) -> ClusterRegistration {
		self.registry
			.register_with_cluster(
				AgentInfo {
					agent_id,
					cluster_name: "victim".to_string(),
					node_name: "node-01".to_string(),
					version: "0.1.0".to_string(),
					last_seen: Utc::now(),
				},
				cluster_id,
			)
			.expect("fresh agent id must register")
	}
}

/// Serve `AgentService` over a fresh registry, with or without the agent JWT
/// interceptor in front of it.
async fn serve(with_interceptor: bool) -> TestServer {
	let registry = Arc::new(AgentRegistry::new());
	let agent_grpc =
		AgentServiceGrpc::new(Arc::new(RegistryBackedAgentService::new(registry.clone())));

	let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
		.await
		.expect("bind ephemeral port");
	let addr = listener.local_addr().expect("local addr");
	let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

	let handle = if with_interceptor {
		tokio::spawn(async move {
			Server::builder()
				.add_service(intercepted(
					AgentServiceServer::new(agent_grpc),
					AgentJwtInterceptor::new(JWT_SECRET),
				))
				.serve_with_incoming(incoming)
				.await
				.expect("test gRPC server failed");
		})
	} else {
		// Misconfiguration: the service is exposed without the interceptor,
		// so no `AgentClaims` ever reach it.
		tokio::spawn(async move {
			Server::builder()
				.add_service(AgentServiceServer::new(agent_grpc))
				.serve_with_incoming(incoming)
				.await
				.expect("test gRPC server failed");
		})
	};

	TestServer {
		addr,
		registry,
		handle,
	}
}

/// Server wired like the dashboard: agent JWT interceptor + registry service.
#[fixture]
async fn server() -> TestServer {
	serve(true).await
}

/// Server exposing `AgentService` without the interceptor.
#[fixture]
async fn server_without_interceptor() -> TestServer {
	serve(false).await
}

/// Attach the agent token of `cluster_id` to `request`.
fn authenticated<T>(message: T, cluster_id: Uuid) -> Request<T> {
	let token = create_agent_token(cluster_id, JWT_SECRET, 1).expect("agent token");
	let mut request = Request::new(message);
	request.metadata_mut().insert(
		"authorization",
		format!("Bearer {token}").parse().expect("ascii header"),
	);
	request
}

fn health_report(agent_id: Uuid, pod_count: u32) -> pb::AgentHealthReport {
	pb::AgentHealthReport {
		agent_id: agent_id.to_string(),
		healthy: true,
		cpu_usage_percent: 1.0,
		memory_usage_percent: 1.0,
		pod_count,
		reported_at: None,
	}
}

fn deploy_status() -> pb::AgentDeployStatus {
	pb::AgentDeployStatus {
		project_name: "web".to_string(),
		success: true,
		message: "deployed".to_string(),
		timestamp: None,
	}
}

fn connected_event(agent_id: Uuid) -> pb::AgentEvent {
	pb::AgentEvent {
		event: Some(pb::agent_event::Event::Connected(pb::AgentConnected {
			agent_id: agent_id.to_string(),
			cluster_name: "announced".to_string(),
			timestamp: None,
		})),
	}
}

/// Open `AgentStream`, announcing `agent_id` first.
///
/// Returns the sender that keeps the event stream open together with the
/// outcome of the call. The server resolves the call only after it has
/// processed the `Connected` event.
async fn open_stream(
	client: &mut AgentServiceClient<Channel>,
	agent_id: Uuid,
	cluster_id: Uuid,
) -> (
	mpsc::Sender<pb::AgentEvent>,
	Result<tonic::Response<tonic::Streaming<pb::AgentCommand>>, tonic::Status>,
) {
	let (tx, rx) = mpsc::channel(8);
	tx.send(connected_event(agent_id))
		.await
		.expect("receiver is alive");
	let result = client
		.agent_stream(authenticated(ReceiverStream::new(rx), cluster_id))
		.await;
	(tx, result)
}

// --- ReportHealth ---

#[rstest]
#[tokio::test]
async fn report_health_for_own_cluster_agent_is_accepted(#[future] server: TestServer) {
	// Arrange
	let server = server.await;
	let cluster = Uuid::now_v7();
	let agent = Uuid::now_v7();
	let _agent_rx = server.connect_agent(agent, cluster);
	let mut client = server.client().await;

	// Act
	let response = client
		.report_health(authenticated(health_report(agent, 7), cluster))
		.await
		.expect("own-cluster health report must be accepted");

	// Assert
	assert!(response.into_inner().success);
	assert_eq!(server.registry.get_health(&agent).unwrap().pod_count, 7);
}

#[rstest]
#[tokio::test]
async fn report_health_for_other_clusters_agent_is_denied_and_registry_unchanged(
	#[future] server: TestServer,
) {
	// Arrange — the victim agent has reported a legitimate health value
	let server = server.await;
	let victim_cluster = Uuid::now_v7();
	let victim_agent = Uuid::now_v7();
	let attacker_cluster = Uuid::now_v7();
	let _victim_rx = server.connect_agent(victim_agent, victim_cluster);
	let mut client = server.client().await;
	client
		.report_health(authenticated(
			health_report(victim_agent, 3),
			victim_cluster,
		))
		.await
		.expect("victim's own report must be accepted");

	// Act — an agent of another cluster names the victim's agent_id
	let status = client
		.report_health(authenticated(
			health_report(victim_agent, 99),
			attacker_cluster,
		))
		.await
		.expect_err("cross-cluster health report must be denied");

	// Assert
	assert_eq!(status.code(), Code::PermissionDenied);
	assert_eq!(status.message(), BINDING_DENIED);
	assert_eq!(
		server.registry.get_health(&victim_agent).unwrap().pod_count,
		3
	);
	assert_eq!(
		server.registry.agents_for_cluster(&victim_cluster),
		vec![victim_agent]
	);
}

#[rstest]
#[tokio::test]
async fn report_health_for_unregistered_agent_is_denied(#[future] server: TestServer) {
	// Arrange
	let server = server.await;
	let unknown_agent = Uuid::now_v7();
	let mut client = server.client().await;

	// Act
	let status = client
		.report_health(authenticated(
			health_report(unknown_agent, 1),
			Uuid::now_v7(),
		))
		.await
		.expect_err("health for an unregistered agent must be denied");

	// Assert
	assert_eq!(status.code(), Code::PermissionDenied);
	assert_eq!(status.message(), BINDING_DENIED);
	assert!(server.registry.get_health(&unknown_agent).is_none());
}

// --- ReportDeployStatus ---

#[rstest]
#[tokio::test]
async fn report_deploy_status_with_valid_agent_token_is_accepted(#[future] server: TestServer) {
	// Arrange
	let server = server.await;
	let mut client = server.client().await;

	// Act
	let response = client
		.report_deploy_status(authenticated(deploy_status(), Uuid::now_v7()))
		.await
		.expect("deploy status from an authenticated agent must be accepted");

	// Assert
	assert!(response.into_inner().success);
}

// --- AgentStream handshake ---

#[rstest]
#[tokio::test]
async fn agent_stream_binds_announced_agent_to_authenticated_cluster(#[future] server: TestServer) {
	// Arrange
	let server = server.await;
	let cluster = Uuid::now_v7();
	let agent = Uuid::now_v7();
	let mut client = server.client().await;

	// Act
	let (_events, result) = open_stream(&mut client, agent, cluster).await;

	// Assert
	assert!(result.is_ok(), "own handshake must be accepted");
	assert_eq!(server.registry.agents_for_cluster(&cluster), vec![agent]);
}

#[rstest]
#[tokio::test]
async fn agent_stream_with_other_clusters_agent_id_is_denied_and_entry_unchanged(
	#[future] server: TestServer,
) {
	// Arrange — the victim agent is connected and routable
	let server = server.await;
	let victim_cluster = Uuid::now_v7();
	let victim_agent = Uuid::now_v7();
	let attacker_cluster = Uuid::now_v7();
	let mut victim_rx = server.connect_agent(victim_agent, victim_cluster);
	let mut client = server.client().await;

	// Act — an agent of another cluster announces the victim's agent_id
	let (_events, result) = open_stream(&mut client, victim_agent, attacker_cluster).await;

	// Assert — denied, and the victim keeps its binding and command channel
	let status = result.expect_err("hijacking handshake must be denied");
	assert_eq!(status.code(), Code::PermissionDenied);
	assert_eq!(status.message(), BINDING_DENIED);
	assert_eq!(
		server.registry.agents_for_cluster(&victim_cluster),
		vec![victim_agent]
	);
	assert_eq!(
		server.registry.agents_for_cluster(&attacker_cluster),
		Vec::<Uuid>::new()
	);
	let command = AgentCommand::Restart {
		project_name: "web".to_string(),
	};
	server
		.registry
		.send_command_to_cluster(&victim_cluster, command.clone())
		.await
		.expect("victim must still be routable");
	assert_eq!(victim_rx.recv().await, Some(command));
}

#[rstest]
#[tokio::test]
async fn reconnect_keeps_new_connection_registered_after_old_stream_ends(
	#[future] server: TestServer,
) {
	// Arrange — the agent connects, then reconnects under the same identity
	let server = server.await;
	let cluster = Uuid::now_v7();
	let agent = Uuid::now_v7();
	let mut client = server.client().await;
	let (old_events, old_result) = open_stream(&mut client, agent, cluster).await;
	let mut old_commands = old_result.expect("first handshake").into_inner();
	let (_new_events, new_result) = open_stream(&mut client, agent, cluster).await;
	let mut new_commands = new_result.expect("reconnect handshake").into_inner();

	// Act — the old stream ends. Replacing the registry entry closed its
	// command channel, so awaiting its end proves the old forward task has
	// run its cleanup; closing the event sender ends the old event pump.
	let old_end = tokio::time::timeout(std::time::Duration::from_secs(5), old_commands.message())
		.await
		.expect("old command stream must end after replacement")
		.expect("old command stream ends cleanly");
	drop(old_events);
	// The event pump has no observable completion signal; a bounded wait
	// gives it time to run its cleanup before the registry is inspected.
	tokio::time::sleep(std::time::Duration::from_millis(200)).await;

	// Assert — the new connection still owns the entry and receives commands
	assert_eq!(old_end, None);
	assert_eq!(server.registry.agents_for_cluster(&cluster), vec![agent]);
	server
		.registry
		.send_command_to_cluster(
			&cluster,
			AgentCommand::Restart {
				project_name: "web".to_string(),
			},
		)
		.await
		.expect("the reconnected agent must stay routable");
	let delivered = tokio::time::timeout(std::time::Duration::from_secs(5), new_commands.message())
		.await
		.expect("command must reach the new stream")
		.expect("new command stream stays open");
	assert_eq!(
		delivered,
		Some(pb::AgentCommand {
			command: Some(pb::agent_command::Command::Restart(pb::RestartCommand {
				project_name: "web".to_string(),
			})),
		})
	);
}

// --- Missing claims (fail closed) ---

/// `AgentService` RPCs reachable without `AgentClaims`.
#[derive(Debug, Clone, Copy)]
enum Rpc {
	ReportHealth,
	ReportDeployStatus,
	AgentStream,
}

#[rstest]
#[case::report_health(Rpc::ReportHealth)]
#[case::report_deploy_status(Rpc::ReportDeployStatus)]
#[case::agent_stream(Rpc::AgentStream)]
#[tokio::test]
async fn calls_without_claims_are_permission_denied(
	#[future] server_without_interceptor: TestServer,
	#[case] rpc: Rpc,
) {
	// Arrange — a victim agent exists, and the caller carries no claims
	let server = server_without_interceptor.await;
	let victim_agent = Uuid::now_v7();
	let victim_cluster = Uuid::now_v7();
	let _victim_rx = server.connect_agent(victim_agent, victim_cluster);
	let mut client = server.client().await;

	// Act
	let code = match rpc {
		Rpc::ReportHealth => client
			.report_health(Request::new(health_report(victim_agent, 99)))
			.await
			.map(|_| ())
			.map_err(|s| s.code()),
		Rpc::ReportDeployStatus => client
			.report_deploy_status(Request::new(deploy_status()))
			.await
			.map(|_| ())
			.map_err(|s| s.code()),
		Rpc::AgentStream => {
			let (tx, rx) = mpsc::channel(8);
			tx.send(connected_event(Uuid::now_v7()))
				.await
				.expect("receiver is alive");
			client
				.agent_stream(Request::new(ReceiverStream::new(rx)))
				.await
				.map(|_| ())
				.map_err(|s| s.code())
		}
	};

	// Assert — refused, nothing registered or recorded
	assert_eq!(code, Err(Code::PermissionDenied));
	assert!(server.registry.get_health(&victim_agent).is_none());
	assert_eq!(server.registry.count(), 1);
}
