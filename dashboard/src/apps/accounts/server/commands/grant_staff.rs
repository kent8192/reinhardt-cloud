//! `manage grant-staff --github-user-id <id> [--revoke]`.

use async_trait::async_trait;
use clap::{Arg, ArgAction, ArgMatches, Command, value_parser};
use reinhardt::commands::{
	CapabilityCommand, CapabilityContext, CapabilityRequirement, CommandError, CommandResult,
};

use crate::apps::accounts::server::commands::runtime::CommandRuntime;
use crate::apps::accounts::services::server::staff::{
	GrantOutcome, RevokeOutcome, StaffError, grant, revoke,
};

/// Grants or revokes Staff for a numeric GitHub user ID.
///
/// Granting works for a GitHub account that has never signed in: the User is
/// created from the ID alone and can then sign in whatever the sign-up policy
/// is (SR-105). Revoking removes Staff and ends the User's sessions; a User who
/// was pre-provisioned and never signed in is removed instead, which ends the
/// exemption with the grant.
#[derive(Debug, Clone, Copy)]
pub struct GrantStaffCommand;

#[async_trait]
impl CapabilityCommand for GrantStaffCommand {
	fn cli(&self) -> Command {
		Command::new("grant-staff")
			.about("Grant Staff to a GitHub account, or revoke it with --revoke")
			.long_about(
				"Grant Staff to the GitHub account with the given numeric user ID. An account \
				 that has never signed in is pre-provisioned: it can sign in whatever the \
				 sign-up policy is. With --revoke, remove Staff and end the User's sessions.",
			)
			.arg(
				Arg::new("github-user-id")
					.long("github-user-id")
					.value_name("ID")
					.required(true)
					.value_parser(value_parser!(i64).range(1..))
					.help("Numeric GitHub user ID (not the login)"),
			)
			.arg(
				Arg::new("revoke")
					.long("revoke")
					.action(ArgAction::SetTrue)
					.help("Remove Staff instead of granting it"),
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

		if matches.get_flag("revoke") {
			let outcome = revoke(github_user_id, &runtime.sessions()?)
				.await
				.map_err(command_error)?;
			println!("{}", describe_revoke(github_user_id, &outcome));
		} else {
			let outcome = grant(github_user_id).await.map_err(command_error)?;
			println!("{}", describe_grant(github_user_id, &outcome));
		}
		Ok(())
	}
}

fn command_error(error: StaffError) -> CommandError {
	CommandError::ExecutionError(error.to_string())
}

fn describe_grant(github_user_id: i64, outcome: &GrantOutcome) -> String {
	match outcome {
		GrantOutcome::PreProvisioned { .. } => format!(
			"GitHub user {github_user_id} is pre-provisioned as Staff and can sign in with GitHub whatever the sign-up policy is."
		),
		GrantOutcome::Granted {
			deactivated: false, ..
		} => format!("GitHub user {github_user_id} is now Staff."),
		GrantOutcome::Granted {
			deactivated: true, ..
		} => format!(
			"GitHub user {github_user_id} is now Staff, but the User is deactivated and cannot sign in."
		),
		GrantOutcome::AlreadyStaff { .. } => {
			format!("GitHub user {github_user_id} already was Staff; nothing changed.")
		}
	}
}

fn describe_revoke(github_user_id: i64, outcome: &RevokeOutcome) -> String {
	match outcome {
		RevokeOutcome::Revoked { sessions_ended, .. } => format!(
			"GitHub user {github_user_id} is no longer Staff; {sessions_ended} session(s) ended."
		),
		RevokeOutcome::PreProvisionRemoved { .. } => format!(
			"GitHub user {github_user_id} was pre-provisioned and had never signed in; the User was removed, so the sign-up exemption is gone."
		),
		RevokeOutcome::NotStaff { sessions_ended, .. } => format!(
			"GitHub user {github_user_id} was not Staff; {sessions_ended} session(s) ended."
		),
	}
}
