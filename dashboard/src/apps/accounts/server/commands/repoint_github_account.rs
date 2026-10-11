//! `manage repoint-github-account --github-user-id <id> --new-github-user-id <id>`.

use async_trait::async_trait;
use clap::{Arg, ArgMatches, Command, value_parser};
use reinhardt::commands::{
	CapabilityCommand, CapabilityContext, CapabilityRequirement, CommandError, CommandResult,
};

use crate::apps::accounts::server::commands::runtime::CommandRuntime;
use crate::apps::accounts::services::server::repoint::repoint;

/// Moves a User to another GitHub account (SR-107).
///
/// The User keeps their Memberships, Roles, and Staff flag; their stored GitHub
/// tokens and unused Login Links are removed and every session ends. A new
/// GitHub user ID that another User already has is refused.
#[derive(Debug, Clone, Copy)]
pub struct RepointGithubAccountCommand;

#[async_trait]
impl CapabilityCommand for RepointGithubAccountCommand {
	fn cli(&self) -> Command {
		let id_arg = |name: &'static str, help: &'static str| {
			Arg::new(name)
				.long(name)
				.value_name("ID")
				.required(true)
				.value_parser(value_parser!(i64).range(1..))
				.help(help)
		};
		Command::new("repoint-github-account")
			.about("Move a User to another GitHub account")
			.long_about(
				"Move the User with the given numeric GitHub user ID to another GitHub account. \
				 The User keeps their Memberships and Roles. Their stored GitHub tokens and \
				 unused Login Links are removed and all of their sessions end. A target ID that \
				 another User already has is refused.",
			)
			.arg(id_arg(
				"github-user-id",
				"Numeric GitHub user ID the User has now",
			))
			.arg(id_arg(
				"new-github-user-id",
				"Numeric GitHub user ID the User should have",
			))
	}

	fn requirements(&self, _matches: &ArgMatches) -> Vec<CapabilityRequirement> {
		Vec::new()
	}

	async fn execute(
		&self,
		matches: &ArgMatches,
		_context: &CapabilityContext,
	) -> CommandResult<()> {
		let get = |name: &str| {
			matches
				.get_one::<i64>(name)
				.copied()
				.ok_or_else(|| CommandError::InvalidArguments(format!("--{name} is required")))
		};
		let from = get("github-user-id")?;
		let to = get("new-github-user-id")?;
		let runtime = CommandRuntime::start().await?;

		let done = repoint(from, to, &runtime.sessions()?)
			.await
			.map_err(|error| CommandError::ExecutionError(error.to_string()))?;
		println!(
			"GitHub user {from} was re-pointed to GitHub user {to}; {} session(s) ended. The User signs in with the new account next; its profile fills in at that sign-in.",
			done.sessions_ended + done.sessions_ended_before
		);
		Ok(())
	}
}
