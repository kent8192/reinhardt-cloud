//! Dashboard-specific management commands.

use async_trait::async_trait;
use reinhardt::BaseUser;
use reinhardt::commands::{BaseCommand, CommandContext, CommandError, CommandResult};
use reinhardt::db::orm::Model;

use crate::apps::auth::models::User;
use crate::apps::auth::services::registration::ensure_personal_organization;
use crate::apps::organizations::models::OrganizationMembership;

const DEFAULT_E2E_USERNAME: &str = "e2e-user";
const DEFAULT_E2E_PASSWORD: &str = "e2e-password-123456";
const DEFAULT_E2E_EMAIL: &str = "e2e@example.test";

/// Seed the deployed dashboard with a deterministic authenticated user.
pub struct SeedSelfDeployUserCommand;

#[async_trait]
impl BaseCommand for SeedSelfDeployUserCommand {
	fn name(&self) -> &str {
		"seed-self-deploy-user"
	}

	fn description(&self) -> &str {
		"Seed the Dashboard self-deploy E2E user and Personal Organization"
	}

	fn requires_system_checks(&self) -> bool {
		false
	}

	async fn execute(&self, ctx: &CommandContext) -> CommandResult<()> {
		initialize_orm_database().await?;

		let username = command_value(
			ctx,
			0,
			"DASHBOARD_SELF_DEPLOY_E2E_USERNAME",
			DEFAULT_E2E_USERNAME,
		);
		let password = command_value(
			ctx,
			1,
			"DASHBOARD_SELF_DEPLOY_E2E_PASSWORD",
			DEFAULT_E2E_PASSWORD,
		);
		let email = command_value(ctx, 2, "DASHBOARD_SELF_DEPLOY_E2E_EMAIL", DEFAULT_E2E_EMAIL);

		let user = upsert_active_user(&username, &password, &email).await?;
		ensure_membership(&user).await?;

		ctx.success(&format!(
			"Seeded Dashboard self-deploy user username={} email={}",
			user.username, user.email
		));
		Ok(())
	}
}

fn command_value(ctx: &CommandContext, index: usize, env_key: &str, default: &str) -> String {
	ctx.arg(index)
		.cloned()
		.or_else(|| std::env::var(env_key).ok())
		.unwrap_or_else(|| default.to_string())
}

async fn initialize_orm_database() -> CommandResult<()> {
	let settings = crate::config::settings::get_settings();
	let env_database_url = std::env::var("DATABASE_URL").ok();
	let url = match env_database_url.as_deref() {
		Some(url) => url.to_string(),
		None => settings
			.core
			.databases
			.get("default")
			.map(|database| database.to_url())
			.ok_or_else(|| {
				CommandError::ExecutionError(
					"database configuration `core.databases.default` not found".to_string(),
				)
			})?,
	};

	if env_database_url.as_deref() != Some(url.as_str()) {
		// SAFETY: This management command runs before spawning application tasks.
		unsafe {
			std::env::set_var("DATABASE_URL", &url);
		}
	}

	reinhardt::db::orm::init_database(&url)
		.await
		.map_err(|e| CommandError::ExecutionError(format!("failed to initialize ORM: {e}")))?;
	Ok(())
}

async fn upsert_active_user(username: &str, password: &str, email: &str) -> CommandResult<User> {
	let existing = User::objects()
		.filter(User::field_username().eq(username.to_string()))
		.first()
		.await
		.map_err(|e| CommandError::ExecutionError(format!("failed to query E2E user: {e}")))?;

	let user_exists = existing.is_some();
	let mut user = existing.unwrap_or_else(|| {
		User::build()
			.username(username.to_string())
			.email(email.to_string())
			.first_name(String::new())
			.last_name(String::new())
			.password_hash(None)
			.is_active(true)
			.is_staff(false)
			.is_superuser(false)
			.finish()
	});

	user.email = email.to_string();
	user.is_active = true;
	user.is_staff = false;
	user.is_superuser = false;
	user.set_password(password).map_err(|e| {
		CommandError::ExecutionError(format!("failed to hash E2E user password: {e}"))
	})?;

	if user_exists {
		User::objects()
			.update(&user)
			.await
			.map_err(|e| CommandError::ExecutionError(format!("failed to update E2E user: {e}")))
	} else {
		User::objects()
			.create(&user)
			.await
			.map_err(|e| CommandError::ExecutionError(format!("failed to create E2E user: {e}")))
	}
}

async fn ensure_membership(user: &User) -> CommandResult<()> {
	let membership = OrganizationMembership::objects()
		.filter(OrganizationMembership::field_user_id().eq(user.id.to_string()))
		.first()
		.await
		.map_err(|e| {
			CommandError::ExecutionError(format!("failed to query E2E user membership: {e}"))
		})?;

	if membership.is_some() {
		return Ok(());
	}

	ensure_personal_organization(user)
		.await
		.map_err(|e| CommandError::ExecutionError(format!("failed to provision E2E org: {e}")))
}
