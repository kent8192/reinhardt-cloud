//! What every accounts command needs before it does its work.

use reinhardt::commands::CommandError;

use crate::apps::accounts::services::server::redis_handle::RedisHandle;
use crate::apps::accounts::services::server::sessions::SessionService;
use crate::config::settings::{ProjectSettings, get_resolved_settings};
use crate::server::initialize_orm_pool;

/// The validated settings and the initialized database pool of one command run.
#[derive(Debug)]
pub struct CommandRuntime {
	settings: ProjectSettings,
}

impl CommandRuntime {
	/// Load the settings through the same validation the server uses (required
	/// secrets, hardened deployed profiles), then connect the ORM pool.
	///
	/// The command driver creates the pool only for built-in commands, so a
	/// registered command creates it here. This also installs the process-wide
	/// log output, so the audit events the command emits reach the operator.
	///
	/// # Errors
	///
	/// Returns an error when the settings are invalid or the database cannot be
	/// reached. The message names the failing setting, never a secret value.
	pub async fn start() -> Result<Self, CommandError> {
		crate::logging::init();
		let settings = get_resolved_settings()
			.map_err(|error| CommandError::ExecutionError(error.to_string()))?
			.into_parts()
			.0;
		initialize_orm_pool(&settings)
			.await
			.map_err(|error| CommandError::ExecutionError(error.to_string()))?;
		Ok(Self { settings })
	}

	/// The validated settings.
	#[must_use]
	pub fn settings(&self) -> &ProjectSettings {
		&self.settings
	}

	/// Browser sessions, for the commands that must end them.
	///
	/// # Errors
	///
	/// Returns an error when the configured Redis URL is not valid.
	pub fn sessions(&self) -> Result<SessionService, CommandError> {
		let redis = RedisHandle::new(&self.settings.redis.url)
			.map_err(|_| CommandError::ExecutionError("the Redis URL is not valid".to_owned()))?;
		Ok(SessionService::new(redis))
	}
}
