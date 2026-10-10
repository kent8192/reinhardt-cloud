//! `manage end-sessions --github-user-id <id>`.

use async_trait::async_trait;
use clap::{Arg, ArgMatches, Command, value_parser};
use reinhardt::commands::{
	CapabilityCommand, CapabilityContext, CapabilityRequirement, CommandError, CommandResult,
};

use crate::apps::accounts::server::commands::runtime::CommandRuntime;
use crate::apps::accounts::services::server::user_recovery::end_sessions;

/// Ends every browser session of a User.
///
/// The recovery tool for a failed session cleanup after a Staff revocation or a
/// re-pointing: run it once Redis is reachable. It exits non-zero when Redis
/// fails, so a script cannot mistake a failure for success.
#[derive(Debug, Clone, Copy)]
pub struct EndSessionsCommand;

#[async_trait]
impl CapabilityCommand for EndSessionsCommand {
	fn cli(&self) -> Command {
		Command::new("end-sessions")
			.about("End every browser session of a User")
			.long_about(
				"End every browser session of the User with the given numeric GitHub user ID. \
				 Use it after a Staff revocation or a re-pointing whose session cleanup failed. \
				 Exits non-zero when the sessions cannot be ended.",
			)
			.arg(
				Arg::new("github-user-id")
					.long("github-user-id")
					.value_name("ID")
					.required(true)
					.value_parser(value_parser!(i64).range(1..))
					.help("Numeric GitHub user ID (not the login)"),
			)
	}

	fn requirements(&self, _matches: &ArgMatches) -> Vec<CapabilityRequirement> {
		Vec::new()
	}

	async fn execute(
		&self,
		matches: &ArgMatches,
		_context: &CapabilityContext,
	) -> CommandResult<()> {
		let github_user_id = *matches
			.get_one::<i64>("github-user-id")
			.ok_or_else(|| CommandError::InvalidArguments("--github-user-id is required".into()))?;
		let runtime = CommandRuntime::start().await?;

		let ended = end_sessions(github_user_id, &runtime.sessions()?)
			.await
			.map_err(|error| CommandError::ExecutionError(error.to_string()))?;
		println!("Ended {ended} session(s) of GitHub user {github_user_id}.");
		Ok(())
	}
}
