//! `manage deactivate-user --github-user-id <id>`.

use async_trait::async_trait;
use clap::{Arg, ArgMatches, Command, value_parser};
use reinhardt::commands::{
	CapabilityCommand, CapabilityContext, CapabilityRequirement, CommandError, CommandResult,
};

use crate::apps::accounts::server::commands::runtime::CommandRuntime;
use crate::apps::accounts::services::server::user_recovery::{
	DeactivateOutcome, SessionsStep, deactivate,
};

/// Deactivates a User, then ends their sessions as far as Redis allows.
///
/// The inactive flag is what stops the User: it is read from the database on
/// every request, so a session is refused even if Redis is down and cannot be
/// deleted. That is why a failure to end the sessions is reported (and audited)
/// without failing the command. Bring the User back with `reactivate-user`.
#[derive(Debug, Clone, Copy)]
pub struct DeactivateUserCommand;

#[async_trait]
impl CapabilityCommand for DeactivateUserCommand {
	fn cli(&self) -> Command {
		Command::new("deactivate-user")
			.about("Deactivate a User and end their sessions")
			.long_about(
				"Deactivate the User with the given numeric GitHub user ID: they can no longer \
				 sign in and every session of theirs is refused on its next request. Their \
				 sessions are then ended on a best-effort basis; if Redis is unreachable the \
				 command still succeeds and says so. A User who is already inactive is left \
				 alone. Use `reactivate-user` to undo it.",
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

		let outcome = deactivate(github_user_id, &runtime.sessions()?)
			.await
			.map_err(|error| CommandError::ExecutionError(error.to_string()))?;
		match outcome {
			DeactivateOutcome::Deactivated {
				sessions: SessionsStep::Ended(ended),
				..
			} => println!("GitHub user {github_user_id} is deactivated; {ended} session(s) ended."),
			DeactivateOutcome::Deactivated {
				sessions: SessionsStep::NotEnded(cause),
				..
			} => {
				println!("GitHub user {github_user_id} is deactivated.");
				eprintln!(
					"The sessions could not be ended ({cause}). The User is inactive, so every session is refused on its next request anyway; run `manage end-sessions --github-user-id {github_user_id}` once Redis is reachable to remove them."
				);
			}
			DeactivateOutcome::Unchanged { .. } => {
				println!("GitHub user {github_user_id} was already inactive; nothing changed.");
			}
		}
		Ok(())
	}
}
