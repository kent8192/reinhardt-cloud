//! `manage reactivate-user --github-user-id <id>`.

use async_trait::async_trait;
use clap::{Arg, ArgMatches, Command, value_parser};
use reinhardt::commands::{
	CapabilityCommand, CapabilityContext, CapabilityRequirement, CommandError, CommandResult,
};

use crate::apps::accounts::server::commands::runtime::CommandRuntime;
use crate::apps::accounts::services::server::user_recovery::{ReactivateOutcome, reactivate};

/// Reactivates a deactivated User after ending every session they had.
///
/// The recovery tool for a User the re-pointing fallback deactivated. Sessions
/// that nobody presented while the User was inactive are still in Redis, and
/// would become valid the moment the User is active again, so they are ended
/// first; if that fails the User stays inactive and the command is refused. It
/// works from a shell, which matters when the deactivated User was the only
/// active Staff and so nobody can reach the admin site.
#[derive(Debug, Clone, Copy)]
pub struct ReactivateUserCommand;

#[async_trait]
impl CapabilityCommand for ReactivateUserCommand {
	fn cli(&self) -> Command {
		Command::new("reactivate-user")
			.about("End every session of a deactivated User, then reactivate them")
			.long_about(
				"Reactivate the deactivated User with the given numeric GitHub user ID. Every \
				 session of the User is ended first, so none that was left behind while the User \
				 was inactive comes back to life. If the sessions cannot be ended the User stays \
				 inactive and the command fails. A User who is already active is left alone.",
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

		let outcome = reactivate(github_user_id, &runtime.sessions()?)
			.await
			.map_err(|error| CommandError::ExecutionError(error.to_string()))?;
		match outcome {
			ReactivateOutcome::Reactivated { sessions_ended, .. } => println!(
				"GitHub user {github_user_id} is active again; {sessions_ended} session(s) ended first."
			),
			ReactivateOutcome::Unchanged { .. } => {
				println!("GitHub user {github_user_id} was already active; nothing changed.");
			}
		}
		Ok(())
	}
}
