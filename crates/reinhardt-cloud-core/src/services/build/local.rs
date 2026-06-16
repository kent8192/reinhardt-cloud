//! Local in-process build service implementation.
//!
//! Simulates a build pipeline by emitting build events through a channel.
//! In production, this would shell out to Docker or Buildpacks.

use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use dashmap::DashMap;
use tokio::sync::mpsc;
use tokio_stream::Stream;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::error::ApiError;
use crate::traits::BuildService;
use reinhardt_cloud_types::build::{BuildEvent, BuildPhase, BuildRequest, BuildStatus};

/// In-memory state for a running or completed build.
#[derive(Clone)]
struct BuildState {
	status: BuildStatus,
	cancel_token: CancellationToken,
}

/// Local build service that manages builds in-process.
///
/// Stores build state in a `DashMap` and streams events via channels.
/// Build execution is simulated with phase transitions.
pub struct LocalBuildService {
	builds: Arc<DashMap<Uuid, BuildState>>,
}

impl LocalBuildService {
	pub fn new() -> Self {
		Self {
			builds: Arc::new(DashMap::new()),
		}
	}
}

impl Default for LocalBuildService {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl BuildService for LocalBuildService {
	async fn start_build(
		&self,
		request: BuildRequest,
	) -> Result<Pin<Box<dyn Stream<Item = Result<BuildEvent, ApiError>> + Send>>, ApiError> {
		let build_id = Uuid::now_v7();
		let cancel_token = CancellationToken::new();
		let now = Utc::now();

		let status = BuildStatus {
			build_id,
			project_name: request.project_name.clone(),
			phase: BuildPhase::Queued,
			completed: false,
			success: None,
			started_at: now,
			completed_at: None,
		};

		self.builds.insert(
			build_id,
			BuildState {
				status: status.clone(),
				cancel_token: cancel_token.clone(),
			},
		);

		let (tx, rx) = mpsc::channel(64);
		let builds = self.builds.clone();
		let project_name = request.project_name.clone();
		let image = request.image.clone();

		tokio::spawn(async move {
			run_build_pipeline(build_id, &project_name, &image, tx, cancel_token, builds).await;
		});

		info!(build_id = %build_id, project = %request.project_name, "Build started");
		Ok(Box::pin(ReceiverStream::new(rx)))
	}

	async fn cancel_build(&self, build_id: Uuid) -> Result<(), ApiError> {
		let state = self
			.builds
			.get(&build_id)
			.ok_or_else(|| ApiError::NotFound(format!("Build {build_id} not found")))?;

		if state.status.completed {
			return Err(ApiError::BadRequest(format!(
				"Build {build_id} already completed"
			)));
		}

		state.cancel_token.cancel();
		warn!(build_id = %build_id, "Build cancelled");
		Ok(())
	}

	async fn get_build_status(&self, build_id: Uuid) -> Result<BuildStatus, ApiError> {
		self.builds
			.get(&build_id)
			.map(|s| s.status.clone())
			.ok_or_else(|| ApiError::NotFound(format!("Build {build_id} not found")))
	}
}

/// Execute the build pipeline, emitting events through the channel.
async fn run_build_pipeline(
	build_id: Uuid,
	project_name: &str,
	image: &str,
	tx: mpsc::Sender<Result<BuildEvent, ApiError>>,
	cancel_token: CancellationToken,
	builds: Arc<DashMap<Uuid, BuildState>>,
) {
	let phases = [
		(BuildPhase::Pulling, "Pulling base image...", 100),
		(BuildPhase::Building, "Building application...", 200),
		(BuildPhase::Pushing, "Pushing image to registry...", 100),
		(BuildPhase::Finalizing, "Finalizing build...", 50),
	];

	for (phase, message, delay_ms) in &phases {
		// Check cancellation before each phase
		if cancel_token.is_cancelled() {
			let _ = tx
				.send(Ok(BuildEvent::Error {
					message: "Build cancelled by user".to_string(),
					timestamp: Utc::now(),
				}))
				.await;
			update_build_completed(&builds, build_id, false);
			return;
		}

		// Emit phase change
		let _ = tx
			.send(Ok(BuildEvent::PhaseChange {
				phase: phase.clone(),
				timestamp: Utc::now(),
			}))
			.await;

		// Update stored status
		if let Some(mut state) = builds.get_mut(&build_id) {
			state.status.phase = phase.clone();
		}

		// Emit log line
		let _ = tx
			.send(Ok(BuildEvent::Log {
				message: message.to_string(),
				timestamp: Utc::now(),
			}))
			.await;

		// Simulate work
		tokio::select! {
			_ = tokio::time::sleep(std::time::Duration::from_millis(*delay_ms)) => {}
			_ = cancel_token.cancelled() => {
				let _ = tx.send(Ok(BuildEvent::Error {
					message: "Build cancelled by user".to_string(),
					timestamp: Utc::now(),
				})).await;
				update_build_completed(&builds, build_id, false);
				return;
			}
		}
	}

	// Emit artifact ready
	let digest = format!("sha256:{:x}", Uuid::now_v7().as_u128());
	let _ = tx
		.send(Ok(BuildEvent::ArtifactReady {
			artifact_url: format!("{image}@{digest}"),
			digest: digest.clone(),
			timestamp: Utc::now(),
		}))
		.await;

	// Emit completion
	let _ = tx
		.send(Ok(BuildEvent::Complete {
			success: true,
			timestamp: Utc::now(),
		}))
		.await;

	update_build_completed(&builds, build_id, true);
	info!(build_id = %build_id, project = project_name, "Build completed successfully");
}

/// Update build state to completed.
fn update_build_completed(builds: &DashMap<Uuid, BuildState>, build_id: Uuid, success: bool) {
	if let Some(mut state) = builds.get_mut(&build_id) {
		state.status.completed = true;
		state.status.success = Some(success);
		state.status.completed_at = Some(Utc::now());
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	use tokio_stream::StreamExt;

	#[rstest]
	#[tokio::test]
	async fn test_start_build_streams_events() {
		// Arrange
		let service = LocalBuildService::new();
		let request = BuildRequest {
			project_name: "test-app".to_string(),
			image: "registry.example.com/test:v1".to_string(),
			env_vars: vec![],
			dockerfile: None,
			context_path: None,
		};

		// Act
		let mut stream = service.start_build(request).await.unwrap();
		let mut events = vec![];
		while let Some(event) = stream.next().await {
			events.push(event.unwrap());
		}

		// Assert — should have phase changes, logs, artifact, and completion
		assert!(events.len() >= 6);
		assert!(matches!(
			events.last().unwrap(),
			BuildEvent::Complete { success: true, .. }
		));
		// Verify all phases are represented
		let phase_changes: Vec<_> = events
			.iter()
			.filter(|e| matches!(e, BuildEvent::PhaseChange { .. }))
			.collect();
		assert_eq!(phase_changes.len(), 4);
	}

	#[rstest]
	#[tokio::test]
	async fn test_get_build_status_after_completion() {
		// Arrange
		let service = LocalBuildService::new();
		let request = BuildRequest {
			project_name: "status-test".to_string(),
			image: "img:latest".to_string(),
			env_vars: vec![],
			dockerfile: None,
			context_path: None,
		};

		// Act — consume the entire stream to completion
		let mut stream = service.start_build(request).await.unwrap();
		let mut build_id = None;
		while let Some(event) = stream.next().await {
			if let Ok(BuildEvent::ArtifactReady { artifact_url, .. }) = &event {
				// Extract build_id from the first build in the map
				if build_id.is_none() {
					build_id = service.builds.iter().next().map(|r| *r.key());
				}
				let _ = artifact_url;
			}
		}

		let bid = build_id.unwrap();
		let status = service.get_build_status(bid).await.unwrap();

		// Assert
		assert!(status.completed);
		assert_eq!(status.success, Some(true));
		assert!(status.completed_at.is_some());
	}

	#[rstest]
	#[tokio::test]
	async fn test_cancel_build() {
		// Arrange
		let service = LocalBuildService::new();
		let request = BuildRequest {
			project_name: "cancel-test".to_string(),
			image: "img:latest".to_string(),
			env_vars: vec![],
			dockerfile: None,
			context_path: None,
		};

		let mut stream = service.start_build(request).await.unwrap();

		// Get the build_id
		let build_id = service.builds.iter().next().map(|r| *r.key()).unwrap();

		// Act — cancel immediately
		service.cancel_build(build_id).await.unwrap();

		// Drain the stream
		let mut events = vec![];
		while let Some(event) = stream.next().await {
			events.push(event.unwrap());
		}

		// Assert — should contain an error event about cancellation
		let has_cancel_error = events.iter().any(
			|e| matches!(e, BuildEvent::Error { message, .. } if message.contains("cancelled")),
		);
		assert!(has_cancel_error);
	}

	#[rstest]
	#[tokio::test]
	async fn test_get_nonexistent_build() {
		// Arrange
		let service = LocalBuildService::new();

		// Act
		let result = service.get_build_status(Uuid::now_v7()).await;

		// Assert
		assert!(result.is_err());
	}
}
