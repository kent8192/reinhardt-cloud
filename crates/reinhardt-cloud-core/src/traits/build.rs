//! Build service trait.

use std::pin::Pin;

use async_trait::async_trait;
use tokio_stream::Stream;
use uuid::Uuid;

use crate::error::ApiError;
use reinhardt_cloud_types::build::{BuildEvent, BuildRequest, BuildStatus};

/// Trait for managing application builds.
///
/// Implementations handle build orchestration, including starting builds
/// that stream events, cancellation, and status queries.
#[async_trait]
pub trait BuildService: Send + Sync + 'static {
	/// Start a new build and return a stream of build events.
	///
	/// The stream emits `BuildEvent` items as the build progresses,
	/// ending with `BuildEvent::Complete`.
	async fn start_build(
		&self,
		request: BuildRequest,
	) -> Result<Pin<Box<dyn Stream<Item = Result<BuildEvent, ApiError>> + Send>>, ApiError>;

	/// Cancel a running build.
	async fn cancel_build(&self, build_id: Uuid) -> Result<(), ApiError>;

	/// Get the current status of a build.
	async fn get_build_status(&self, build_id: Uuid) -> Result<BuildStatus, ApiError>;
}
