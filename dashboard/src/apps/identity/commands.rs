//! Installation bootstrap through the Reinhardt management command registry.

use clap::{Arg, ArgMatches, Command};
use reinhardt::auth::{Argon2Hasher, PasswordHasher};
use reinhardt::commands::{
	CapabilityCommand, CapabilityContext, CapabilityRequirement, CommandError, CommandResult,
	SelectedDatabase,
};
use reinhardt::core::validators::{EmailValidator, Validator};
use reinhardt::db::orm::{Model, connection::DatabaseConnectionLease};

use super::models::UserAccount;
use crate::apps::organization::persistence::create_personal_organization;

pub struct CreatePlatformAdmin;

#[reinhardt::core::async_trait]
impl CapabilityCommand for CreatePlatformAdmin {
	fn cli(&self) -> Command {
		Command::new("createplatformadmin")
			.about("Create a platform administrator and personal organization")
			.arg(
				Arg::new("email")
					.required(true)
					.help("Administrator email address"),
			)
	}
	fn requirements(&self, _matches: &ArgMatches) -> Vec<CapabilityRequirement> {
		vec![CapabilityRequirement::settings::<SelectedDatabase>(None)]
	}
	async fn execute(&self, matches: &ArgMatches, ctx: &CapabilityContext) -> CommandResult<()> {
		let email = matches
			.get_one::<String>("email")
			.ok_or_else(|| CommandError::InvalidArguments("An email address is required".into()))?
			.trim()
			.to_ascii_lowercase();
		let password = std::env::var("CLOUD_BOOTSTRAP_PASSWORD").map_err(|_| {
			CommandError::InvalidArguments("Set CLOUD_BOOTSTRAP_PASSWORD for this command".into())
		})?;
		if EmailValidator::new().validate(email.as_str()).is_err()
			|| email.len() > 254
			|| !(12..=1024).contains(&password.len())
		{
			return Err(CommandError::InvalidArguments(
				"Use a valid email and a password of 12 to 1024 bytes".into(),
			));
		}
		let database = ctx.settings::<SelectedDatabase>(None)?;
		let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| database.url());
		let backend = reinhardt::db::backends::DatabaseConnection::connect_postgres_with_pool_size(
			&url,
			Some(2),
		)
		.await
		.map_err(|_| CommandError::ExecutionError("Cannot initialize identity storage".into()))?;
		let lease = DatabaseConnectionLease::register(backend)
			.map_err(|_| CommandError::ExecutionError("Cannot register identity storage".into()))?;
		let hash = tokio::task::spawn_blocking(move || Argon2Hasher.hash(&password))
			.await
			.map_err(|_| CommandError::ExecutionError("Password processing failed".into()))?
			.map_err(|_| CommandError::ExecutionError("Password processing failed".into()))?;
		let user = UserAccount::new()
			.email(&email)
			.password_hash(hash)
			.active(true)
			.platform_admin(true)
			.finish();
		let connection = lease.handle();
		let result: reinhardt::core::exception::Result<()> = connection
			.atomic(async |transaction| {
				let user = UserAccount::objects()
					.create_with_conn(transaction, &user)
					.await?;
				let name = email.chars().take(200).collect::<String>();
				create_personal_organization(transaction, user.id, &name).await?;
				Ok(())
			})
			.await;
		result.map_err(|_| {
			CommandError::ExecutionError(
				"Account creation failed; verify migrations and email uniqueness".into(),
			)
		})?;
		println!("Platform administrator and personal organization created");
		Ok(())
	}
}
