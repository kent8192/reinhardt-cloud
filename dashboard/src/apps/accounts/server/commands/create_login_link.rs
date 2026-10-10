//! `manage create-login-link --github-user-id <id> [--ttl-minutes <n>]`.

use std::time::Duration;

use async_trait::async_trait;
use clap::{Arg, ArgMatches, Command, value_parser};
use reinhardt::commands::{
	CapabilityCommand, CapabilityContext, CapabilityRequirement, CommandError, CommandResult,
};

use crate::apps::accounts::server::commands::runtime::CommandRuntime;
use crate::apps::accounts::services::server::login_links::{DEFAULT_LIFETIME, MAX_LIFETIME, issue};
use crate::apps::accounts::urls::paths::LOGIN_LINK_PAGE_PATH;

/// Issues a Login Link for an existing, active User.
///
/// The URL is the only thing written to standard output, once, so that
/// `url=$(manage create-login-link ...)` captures exactly the link. Everything
/// else goes to standard error. The secret is not stored (only its digest is)
/// and is not logged, so it cannot be shown again.
#[derive(Debug, Clone, Copy)]
pub struct CreateLoginLinkCommand;

fn max_minutes() -> u64 {
	MAX_LIFETIME.as_secs() / 60
}

fn default_minutes() -> u64 {
	DEFAULT_LIFETIME.as_secs() / 60
}

#[async_trait]
impl CapabilityCommand for CreateLoginLinkCommand {
	fn cli(&self) -> Command {
		Command::new("create-login-link")
			.about("Issue a single-use sign-in URL for an existing User")
			.long_about(format!(
				"Issue a single-use, short-lived Login Link for the User with the given numeric \
				 GitHub user ID. The User must exist and be active: a Login Link never creates \
				 one. The URL is printed once to standard output and cannot be shown again. It \
				 is valid for at most {} minutes.",
				max_minutes()
			))
			.arg(
				Arg::new("github-user-id")
					.long("github-user-id")
					.value_name("ID")
					.required(true)
					.value_parser(value_parser!(i64).range(1..))
					.help("Numeric GitHub user ID of the User to sign in (not the login)"),
			)
			.arg(
				Arg::new("ttl-minutes")
					.long("ttl-minutes")
					.value_name("MINUTES")
					.value_parser(value_parser!(u64).range(1..=max_minutes()))
					.help(format!(
						"How long the link stays valid, 1 to {} (default {})",
						max_minutes(),
						default_minutes()
					)),
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
		let minutes = *matches
			.get_one::<u64>("ttl-minutes")
			.unwrap_or(&default_minutes());
		let runtime = CommandRuntime::start().await?;

		// Without a public origin there is no URL to hand out (it is empty when
		// GitHub sign-in is disabled and nothing else needed it). Refuse before
		// anything is stored.
		let origin = runtime.settings().accounts.public_origin().ok_or_else(|| {
			CommandError::ExecutionError(
				"REINHARDT_CLOUD_PUBLIC_URL must be set to the origin the Dashboard is served from (for example https://cloud.example.com), without a path".to_owned(),
			)
		})?;

		let link = issue(github_user_id, Duration::from_secs(minutes * 60))
			.await
			.map_err(|error| CommandError::ExecutionError(error.to_string()))?;

		eprintln!(
			"Login Link for GitHub user {github_user_id}, valid until {} UTC. It can be used once and cannot be shown again:",
			link.expires_at.format("%Y-%m-%d %H:%M:%S")
		);
		// The secret travels in the URL fragment, which browsers never send to the
		// server (see `LOGIN_LINK_PAGE_PATH`).
		println!("{origin}{LOGIN_LINK_PAGE_PATH}#{}", link.secret.expose());
		Ok(())
	}
}
