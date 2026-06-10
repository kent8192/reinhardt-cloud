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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitHubProjectInfo {
	pub id: i64,
	pub repository_id: i64,
	pub deployment_id: i64,
	pub project_name: String,
	pub production_branch: String,
	pub status: String,
}

#[cfg(native)]
async fn current_org_id(user: &crate::apps::auth::models::User) -> Result<i64, ServerFnError> {
	crate::apps::organizations::helpers::current_organization_id_for_user(user.id)
		.await
		.map_err(|e| ServerFnError::application(e.to_string()))
}

#[cfg(native)]
async fn ensure_github_account_linked(
	user: &crate::apps::auth::models::User,
) -> Result<(), ServerFnError> {
	use reinhardt::Model;

	let linked = crate::apps::auth::models::SocialAccount::objects()
		.filter(crate::apps::auth::models::SocialAccount::field_user_id().eq(user.id.to_string()))
		.filter(crate::apps::auth::models::SocialAccount::field_provider().eq("github"))
		.exists()
		.await
		.map_err(|e| {
			ServerFnError::application(format!("Failed to check linked GitHub account: {e}"))
		})?;
	if linked {
		Ok(())
	} else {
		Err(ServerFnError::server(
			403,
			"GitHub account must be linked before importing repositories",
		))
	}
}

#[cfg(native)]
async fn rollback_created_deployment(deployment_id: i64) {
	use reinhardt::Model;

	if let Err(delete_err) = crate::apps::deployments::models::Deployment::objects()
		.delete(deployment_id)
		.await
	{
		tracing::warn!(
			"Failed to roll back deployment {deployment_id} after GitHub import persistence error: {delete_err}"
		);
	}
}

#[cfg(native)]
async fn agent_registry()
-> Result<std::sync::Arc<reinhardt_cloud_grpc::registry::AgentRegistry>, ServerFnError> {
	use reinhardt::di::{ContextLevel, get_di_context};

	let ctx = get_di_context(ContextLevel::Root);
	let registry = ctx
		.resolve::<crate::config::grpc::AgentRegistrySingleton>()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to resolve AgentRegistry: {e}")))?;
	Ok(registry.0.clone())
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

#[cfg(native)]
pub(crate) fn github_project_info(
	project: crate::apps::github::models::GitHubProject,
) -> GitHubProjectInfo {
	GitHubProjectInfo {
		id: project.id.unwrap_or_default(),
		repository_id: *project.repository_id(),
		deployment_id: *project.deployment_id(),
		project_name: project.project_name,
		production_branch: project.production_branch,
		status: project.status,
	}
}

#[cfg(native)]
pub(crate) fn repository_from_installation_repository(
	installation_row_id: i64,
	repository: crate::apps::github::services::client::GitHubInstallationRepository,
) -> crate::apps::github::models::GitHubRepository {
	crate::apps::github::models::GitHubRepository::build()
		.installation(installation_row_id)
		.github_repository_id(repository.id)
		.full_name(repository.full_name)
		.owner_login(repository.owner_login)
		.name(repository.name)
		.default_branch(repository.default_branch)
		.private(repository.private)
		.selected(false)
		.finish()
}

#[cfg(native)]
async fn sync_repositories_for_installation(
	installation: &crate::apps::github::models::GitHubInstallation,
) -> Result<(), ServerFnError> {
	use crate::apps::github::services::client::{GitHubAppClient, ReqwestGitHubAppClient};
	use crate::apps::github::services::config::GitHubAppSettings;
	use reinhardt::Model;

	let installation_row_id = installation
		.id
		.ok_or_else(|| ServerFnError::application("GitHub installation row missing primary key"))?;
	let settings = GitHubAppSettings::from_env()
		.map_err(|e| ServerFnError::application(format!("GitHub App settings invalid: {e}")))?;
	let client = ReqwestGitHubAppClient::new(settings);
	let repositories = client
		.list_repositories(installation.installation_id)
		.await
		.map_err(|e| {
			ServerFnError::application(format!("Failed to sync GitHub repositories: {e}"))
		})?;
	for repository in repositories {
		let mut next = repository_from_installation_repository(installation_row_id, repository);
		let existing = crate::apps::github::models::GitHubRepository::objects()
			.filter(
				crate::apps::github::models::GitHubRepository::field_github_repository_id()
					.eq(next.github_repository_id),
			)
			.first()
			.await
			.map_err(|e| {
				ServerFnError::application(format!("Failed to load cached GitHub repository: {e}"))
			})?;
		if let Some(mut existing) = existing {
			next.id = existing.id;
			next.selected = existing.selected;
			existing.installation = next.installation;
			existing.full_name = next.full_name;
			existing.owner_login = next.owner_login;
			existing.name = next.name;
			existing.default_branch = next.default_branch;
			existing.private = next.private;
			crate::apps::github::models::GitHubRepository::objects()
				.update(&existing)
				.await
				.map_err(|e| {
					ServerFnError::application(format!(
						"Failed to update cached GitHub repository: {e}"
					))
				})?;
		} else {
			crate::apps::github::models::GitHubRepository::objects()
				.create(&next)
				.await
				.map_err(|e| {
					ServerFnError::application(format!("Failed to cache GitHub repository: {e}"))
				})?;
		}
	}
	Ok(())
}

#[server_fn]
pub async fn list_github_installations_for_current_org(
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> Result<Vec<GitHubInstallationInfo>, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		ensure_github_account_linked(&user).await?;
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
pub async fn import_github_repository_for_current_org(
	repository_id: String,
	cluster_id: String,
	project_name: String,
	registry: String,
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> Result<GitHubProjectInfo, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		ensure_github_account_linked(&user).await?;
		let organization_id = current_org_id(&user).await?;
		let repository_id: i64 = repository_id
			.parse()
			.map_err(|_| ServerFnError::application("Invalid repository_id"))?;
		let cluster_id: i64 = cluster_id
			.parse()
			.map_err(|_| ServerFnError::application("Invalid cluster_id"))?;
		let cluster = crate::apps::clusters::models::Cluster::objects()
			.filter(crate::apps::clusters::models::Cluster::field_id().eq(cluster_id))
			.filter(
				crate::apps::clusters::models::Cluster::field_organization_id().eq(organization_id),
			)
			.filter(crate::apps::clusters::models::Cluster::field_is_active().eq(true))
			.first()
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to load cluster: {e}")))?
			.ok_or_else(|| ServerFnError::server(404, "Cluster not found"))?;

		let mut repository = crate::apps::github::models::GitHubRepository::objects()
			.filter(crate::apps::github::models::GitHubRepository::field_id().eq(repository_id))
			.first()
			.await
			.map_err(|e| {
				ServerFnError::application(format!("Failed to load GitHub repository: {e}"))
			})?
			.ok_or_else(|| ServerFnError::server(404, "GitHub repository not found"))?;
		let installation = crate::apps::github::models::GitHubInstallation::objects()
			.filter(
				crate::apps::github::models::GitHubInstallation::field_id()
					.eq(*repository.installation_id()),
			)
			.filter(
				crate::apps::github::models::GitHubInstallation::field_organization_id()
					.eq(organization_id),
			)
			.first()
			.await
			.map_err(|e| {
				ServerFnError::application(format!("Failed to load GitHub installation: {e}"))
			})?
			.ok_or_else(|| ServerFnError::server(404, "GitHub repository not found"))?;
		let existing = crate::apps::github::models::GitHubProject::objects()
			.filter(
				crate::apps::github::models::GitHubProject::field_repository_id().eq(repository_id),
			)
			.exists()
			.await
			.map_err(|e| {
				ServerFnError::application(format!("Failed to check GitHub project: {e}"))
			})?;
		if existing {
			return Err(ServerFnError::server(
				409,
				"GitHub repository is already imported",
			));
		}

		let mut import_spec = crate::apps::github::services::import::import_spec_from_repository(
			&repository,
			&project_name,
			&registry,
		)
		.map_err(|e| ServerFnError::server(400, e))?;
		let settings = crate::apps::github::services::config::GitHubAppSettings::from_env()
			.map_err(|e| ServerFnError::application(format!("GitHub App settings invalid: {e}")))?;
		let client = crate::apps::github::services::client::ReqwestGitHubAppClient::new(settings);
		let installation_token = client
			.create_installation_access_token(installation.installation_id)
			.await
			.map_err(|e| {
				ServerFnError::application(format!(
					"Failed to create GitHub installation access token: {e}"
				))
			})?;
		let pipeline_input = crate::apps::github::services::pipeline::GitHubDeployPipelineInput {
			installation_id: installation.installation_id,
			full_name: repository.full_name.clone(),
			branch: repository.default_branch.clone(),
			project_name: import_spec.project_name.clone(),
			namespace: import_spec.namespace.clone(),
			registry: import_spec.registry.clone(),
			private: repository.private,
		};
		let pipeline_output = crate::apps::github::services::pipeline::run_github_deploy_pipeline(
			&pipeline_input,
			&installation_token.token,
		)
		.await
		.map_err(|e| ServerFnError::application(format!("GitHub deploy pipeline failed: {e}")))?;
		crate::apps::github::services::import::enrich_import_spec(
			&mut import_spec,
			pipeline_output.introspect,
			pipeline_output.credentials_secret,
		);
		let manifest =
			crate::apps::github::services::import::source_project_yaml(&import_spec)
				.map_err(|e| ServerFnError::server(400, e))?;
		let agent_registry = agent_registry().await?;
		if let Some(secret_name) = import_spec.credentials_secret.as_deref() {
			crate::apps::github::services::deploy::send_git_credentials_secret_to_cluster(
				&agent_registry,
				&cluster,
				&import_spec.project_name,
				&import_spec.namespace,
				secret_name,
				&installation_token.token,
			)
			.await
			.map_err(|e| {
				ServerFnError::application(format!(
					"Failed to apply GitHub repository credentials: {e}"
				))
			})?;
		}
		crate::apps::github::services::deploy::send_project_apply_to_cluster(
			&agent_registry,
			&cluster,
			&import_spec.project_name,
			&manifest,
		)
		.await
		.map_err(|e| {
			ServerFnError::application(format!("Failed to apply Project manifest: {e}"))
		})?;
		let deployment = crate::apps::deployments::models::Deployment::build()
			.organization(organization_id)
			.project_name(import_spec.project_name.clone())
			.cluster(cluster_id)
			.status("pending".to_string())
			.image(format!("{}:pending", import_spec.registry))
			.project_yaml(Some(manifest))
			.finish();
		let deployment = crate::apps::deployments::models::Deployment::objects()
			.create(&deployment)
			.await
			.map_err(|e| ServerFnError::application(format!("Failed to create deployment: {e}")))?;
		let deployment_id = deployment.id.ok_or_else(|| {
			ServerFnError::application("Deployment row missing primary key after insert")
		})?;
		let repository_row_id = repository.id.ok_or_else(|| {
			ServerFnError::application("GitHub repository row missing primary key")
		})?;
		let production_branch = import_spec.branch.clone();
		let project = crate::apps::github::models::GitHubProject::build()
			.organization(organization_id)
			.repository(repository_row_id)
			.deployment(deployment_id)
			.project_name(import_spec.project_name)
			.production_branch(production_branch)
			.status("imported".to_string())
			.finish();
		let project = match crate::apps::github::models::GitHubProject::objects()
			.create(&project)
			.await
		{
			Ok(project) => project,
			Err(e) => {
				rollback_created_deployment(deployment_id).await;
				return Err(ServerFnError::application(format!(
					"Failed to create GitHub project: {e}"
				)));
			}
		};
		repository.selected = true;
		if let Err(e) = crate::apps::github::models::GitHubRepository::objects()
			.update(&repository)
			.await
		{
			rollback_created_deployment(deployment_id).await;
			return Err(ServerFnError::application(format!(
				"Failed to update GitHub repository: {e}"
			)));
		}
		Ok(github_project_info(project))
	}
	#[cfg(wasm)]
	{
		let _ = (repository_id, cluster_id, project_name, registry, user);
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn list_github_repositories_for_current_org(
	#[inject] CurrentUser(user): CurrentUser<crate::apps::auth::models::User>,
) -> Result<Vec<GitHubRepositoryInfo>, ServerFnError> {
	#[cfg(native)]
	{
		use reinhardt::Model;

		ensure_github_account_linked(&user).await?;
		let organization_id = current_org_id(&user).await?;
		let installations = crate::apps::github::models::GitHubInstallation::objects()
			.filter(
				crate::apps::github::models::GitHubInstallation::field_organization_id()
					.eq(organization_id),
			)
			.filter(crate::apps::github::models::GitHubInstallation::field_status().eq("active"))
			.order_by(&["account_login", "id"])
			.all()
			.await
			.map_err(|e| {
				ServerFnError::application(format!("Failed to list GitHub installations: {e}"))
			})?;
		let mut repositories = Vec::new();
		for installation in installations {
			let row_id = installation.id.ok_or_else(|| {
				ServerFnError::application("GitHub installation row missing primary key")
			})?;
			sync_repositories_for_installation(&installation).await?;
			let mut current = crate::apps::github::models::GitHubRepository::objects()
				.filter(
					crate::apps::github::models::GitHubRepository::field_installation_id()
						.eq(row_id),
				)
				.order_by(&["full_name", "id"])
				.all()
				.await
				.map_err(|e| {
					ServerFnError::application(format!("Failed to list GitHub repositories: {e}"))
				})?;
			repositories.append(&mut current);
		}
		repositories.sort_by(|a, b| a.full_name.cmp(&b.full_name).then(a.id.cmp(&b.id)));
		Ok(repositories
			.into_iter()
			.map(github_repository_info)
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

		ensure_github_account_linked(&user).await?;
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
		sync_repositories_for_installation(&installation).await?;
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
