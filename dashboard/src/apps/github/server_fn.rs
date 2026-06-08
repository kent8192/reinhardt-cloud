//! GitHub App server functions for the WASM dashboard.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};
use serde::{Deserialize, Serialize};

#[cfg(native)]
use reinhardt::CurrentUser;
#[cfg(wasm)]
// CurrentUser is a WASM placeholder for the `#[server_fn]` signature; native
// builds resolve the real injected user type.
#[allow(dead_code)]
struct CurrentUser<U>(pub U);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitHubInstallationInfo {
	pub id: i64,
	pub installation_id: i64,
	pub account_login: String,
	pub account_type: String,
	pub status: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitHubRepositoryInfo {
	pub id: i64,
	pub github_repository_id: i64,
	pub full_name: String,
	pub owner_login: String,
	pub name: String,
	pub default_branch: String,
	pub private: bool,
	pub selected: bool,
}

#[cfg(native)]
async fn current_org_id(user: &crate::apps::auth::models::User) -> Result<i64, ServerFnError> {
	crate::apps::organizations::helpers::current_organization_id_for_user(user.id)
		.await
		.map_err(|e| ServerFnError::application(e.to_string()))
}

#[cfg(native)]
pub(crate) fn github_installation_info(
	installation: crate::apps::github::models::GitHubInstallation,
) -> GitHubInstallationInfo {
	GitHubInstallationInfo {
		id: installation.id.unwrap_or_default(),
		installation_id: installation.installation_id,
		account_login: installation.account_login,
		account_type: installation.account_type,
		status: installation.status,
	}
}

#[cfg(native)]
pub(crate) fn github_repository_info(
	repository: crate::apps::github::models::GitHubRepository,
) -> GitHubRepositoryInfo {
	GitHubRepositoryInfo {
		id: repository.id.unwrap_or_default(),
		github_repository_id: repository.github_repository_id,
		full_name: repository.full_name,
		owner_login: repository.owner_login,
		name: repository.name,
		default_branch: repository.default_branch,
		private: repository.private,
		selected: repository.selected,
	}
}

#[server_fn]
pub async fn list_github_installations_for_current_org(
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> Result<Vec<GitHubInstallationInfo>, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		let organization_id = current_org_id(&user).await?;
		let installations = crate::apps::github::models::GitHubInstallation::objects()
			.filter(
				crate::apps::github::models::GitHubInstallation::field_organization_id()
					.eq(organization_id),
			)
			.order_by(&["account_login", "id"])
			.all()
			.await
			.map_err(|e| {
				ServerFnError::application(format!("Failed to list GitHub installations: {e}"))
			})?;
		Ok(installations
			.into_iter()
			.map(github_installation_info)
			.collect())
	}
	#[cfg(wasm)]
	{
		let _ = user;
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn list_github_repositories_for_installation(
	installation_id: i64,
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> Result<Vec<GitHubRepositoryInfo>, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		let organization_id = current_org_id(&user).await?;
		let installation = crate::apps::github::models::GitHubInstallation::objects()
			.filter(crate::apps::github::models::GitHubInstallation::field_id().eq(installation_id))
			.filter(
				crate::apps::github::models::GitHubInstallation::field_organization_id()
					.eq(organization_id),
			)
			.first()
			.await
			.map_err(|e| {
				ServerFnError::application(format!("Failed to load GitHub installation: {e}"))
			})?
			.ok_or_else(|| ServerFnError::server(404, "GitHub installation not found"))?;
		let row_id = installation.id.ok_or_else(|| {
			ServerFnError::application("GitHub installation row missing primary key")
		})?;
		let repositories = crate::apps::github::models::GitHubRepository::objects()
			.filter(
				crate::apps::github::models::GitHubRepository::field_installation_id().eq(row_id),
			)
			.order_by(&["full_name", "id"])
			.all()
			.await
			.map_err(|e| {
				ServerFnError::application(format!("Failed to list GitHub repositories: {e}"))
			})?;
		Ok(repositories
			.into_iter()
			.map(github_repository_info)
			.collect())
	}
	#[cfg(wasm)]
	{
		let _ = (installation_id, user);
		unreachable!("server_fn body is replaced on wasm")
	}
}
