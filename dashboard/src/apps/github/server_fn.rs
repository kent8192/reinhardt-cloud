//! GitHub App server functions for the WASM dashboard.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};
use serde::{Deserialize, Serialize};

use crate::apps::deployments::server_fn::ProjectPreviewSummary;

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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitHubOnboardingInfo {
	pub github_account_linked: bool,
	pub install_url: Option<String>,
}

#[cfg(native)]
async fn current_org_id(user: &crate::apps::auth::models::User) -> Result<i64, ServerFnError> {
	crate::apps::organizations::helpers::current_organization_id_for_user(user.id)
		.await
		.map_err(|e| ServerFnError::application(e.to_string()))
}

#[cfg(native)]
async fn github_account_linked(
	user: &crate::apps::auth::models::User,
) -> Result<bool, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::auth::models::SocialAccount;

	SocialAccount::objects()
		.filter(SocialAccount::field_user_id().eq(user.id.to_string()))
		.filter(SocialAccount::field_provider().eq("github"))
		.exists()
		.await
		.map_err(|e| {
			ServerFnError::application(format!("Failed to check linked GitHub account: {e}"))
		})
}

#[cfg(native)]
async fn ensure_github_account_linked(
	user: &crate::apps::auth::models::User,
) -> Result<(), ServerFnError> {
	let linked = github_account_linked(user).await?;
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

	use crate::apps::deployments::models::Deployment;

	if let Err(delete_err) = Deployment::objects().delete(deployment_id).await {
		tracing::warn!(
			"Failed to roll back deployment {deployment_id} after GitHub import persistence error: {delete_err}"
		);
	}
}

#[cfg(native)]
async fn rollback_created_github_project(project_id: i64) {
	use reinhardt::Model;

	use crate::apps::github::models::GitHubProject;

	if let Err(delete_err) = GitHubProject::objects().delete(project_id).await {
		tracing::warn!(
			"Failed to roll back GitHub project {project_id} after repository update error: {delete_err}"
		);
	}
}

#[cfg(native)]
async fn agent_registry()
-> Result<std::sync::Arc<reinhardt_cloud_grpc::registry::AgentRegistry>, ServerFnError> {
	use reinhardt::di::{ContextLevel, FactoryOutput, get_di_context};

	use crate::config::{AgentRegistrySingleton, AgentRegistrySingletonKey};

	let ctx = get_di_context(ContextLevel::Root);
	let registry = ctx
		.resolve::<FactoryOutput<AgentRegistrySingletonKey, AgentRegistrySingleton>>()
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
	use crate::apps::github::models::GitHubRepository;

	GitHubRepository::build()
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
	use reinhardt::Model;

	use crate::apps::github::models::GitHubRepository;
	use crate::apps::github::services::GitHubAppSettings;
	use crate::apps::github::services::client::{GitHubAppClient, ReqwestGitHubAppClient};

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
		let existing = GitHubRepository::objects()
			.filter(GitHubRepository::field_github_repository_id().eq(next.github_repository_id))
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
			GitHubRepository::objects()
				.update(&existing)
				.await
				.map_err(|e| {
					ServerFnError::application(format!(
						"Failed to update cached GitHub repository: {e}"
					))
				})?;
		} else {
			GitHubRepository::objects()
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
pub async fn get_github_onboarding_for_current_org(
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<GitHubOnboardingInfo, ServerFnError> {
	#[cfg(native)]
	{
		use crate::apps::github::services::GitHubAppSettings;

		Ok(GitHubOnboardingInfo {
			github_account_linked: github_account_linked(&user).await?,
			install_url: GitHubAppSettings::install_url_from_env(),
		})
	}
	#[cfg(wasm)]
	{
		let _ = user;
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn list_github_installations_for_current_org(
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<Vec<GitHubInstallationInfo>, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::github::models::GitHubInstallation;

	ensure_github_account_linked(&user).await?;
	let organization_id = current_org_id(&user).await?;
	let installations = GitHubInstallation::objects()
		.filter(GitHubInstallation::field_organization_id().eq(organization_id))
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

#[server_fn]
pub async fn import_github_repository_for_current_org(
	repository_id: String,
	cluster_id: String,
	project_name: String,
	registry: String,
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<GitHubProjectInfo, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::clusters::models::Cluster;
	use crate::apps::deployments::models::Deployment;
	use crate::apps::github::models::{GitHubInstallation, GitHubProject, GitHubRepository};
	use crate::apps::github::services::GitHubAppSettings;
	use crate::apps::github::services::client::ReqwestGitHubAppClient;
	use crate::apps::github::services::deploy::{
		send_git_credentials_secret_to_cluster, send_project_apply_to_cluster,
	};
	use crate::apps::github::services::import::{
		enrich_import_spec, import_spec_from_repository, source_project_yaml,
	};
	use crate::apps::github::services::pipeline::{
		GitHubDeployPipelineInput, run_github_deploy_pipeline,
	};

	ensure_github_account_linked(&user).await?;
	let organization_id = current_org_id(&user).await?;
	let repository_id: i64 = repository_id
		.parse()
		.map_err(|_| ServerFnError::application("Invalid repository_id"))?;
	let cluster_id: i64 = cluster_id
		.parse()
		.map_err(|_| ServerFnError::application("Invalid cluster_id"))?;
	let cluster = Cluster::objects()
		.filter(Cluster::field_id().eq(cluster_id))
		.filter(Cluster::field_organization_id().eq(organization_id))
		.filter(Cluster::field_is_active().eq(true))
		.first()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to load cluster: {e}")))?
		.ok_or_else(|| ServerFnError::server(404, "Cluster not found"))?;

	let mut repository = GitHubRepository::objects()
		.filter(GitHubRepository::field_id().eq(repository_id))
		.first()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to load GitHub repository: {e}")))?
		.ok_or_else(|| ServerFnError::server(404, "GitHub repository not found"))?;
	let installation = GitHubInstallation::objects()
		.filter(GitHubInstallation::field_id().eq(*repository.installation_id()))
		.filter(GitHubInstallation::field_organization_id().eq(organization_id))
		.first()
		.await
		.map_err(|e| {
			ServerFnError::application(format!("Failed to load GitHub installation: {e}"))
		})?
		.ok_or_else(|| ServerFnError::server(404, "GitHub repository not found"))?;
	let existing = GitHubProject::objects()
		.filter(GitHubProject::field_repository_id().eq(repository_id))
		.exists()
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to check GitHub project: {e}")))?;
	if existing {
		return Err(ServerFnError::server(
			409,
			"GitHub repository is already imported",
		));
	}

	let mut import_spec = import_spec_from_repository(&repository, &project_name, &registry)
		.map_err(|e| ServerFnError::server(400, e))?;
	let settings = GitHubAppSettings::from_env()
		.map_err(|e| ServerFnError::application(format!("GitHub App settings invalid: {e}")))?;
	let client = ReqwestGitHubAppClient::new(settings);
	let installation_token = client
		.create_installation_access_token(installation.installation_id)
		.await
		.map_err(|e| {
			ServerFnError::application(format!(
				"Failed to create GitHub installation access token: {e}"
			))
		})?;
	let pipeline_input = GitHubDeployPipelineInput {
		installation_id: installation.installation_id,
		full_name: repository.full_name.clone(),
		branch: repository.default_branch.clone(),
		project_name: import_spec.project_name.clone(),
		namespace: import_spec.namespace.clone(),
		registry: import_spec.registry.clone(),
		private: repository.private,
	};
	let pipeline_output = run_github_deploy_pipeline(&pipeline_input, &installation_token.token)
		.await
		.map_err(|e| ServerFnError::application(format!("GitHub deploy pipeline failed: {e}")))?;
	enrich_import_spec(
		&mut import_spec,
		pipeline_output.introspect,
		pipeline_output.credentials_secret,
	);
	let manifest = source_project_yaml(&import_spec).map_err(|e| ServerFnError::server(400, e))?;
	let agent_registry = agent_registry().await?;
	if let Some(secret_name) = import_spec.credentials_secret.as_deref() {
		send_git_credentials_secret_to_cluster(
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
	send_project_apply_to_cluster(
		&agent_registry,
		&cluster,
		&import_spec.project_name,
		&manifest,
	)
	.await
	.map_err(|e| ServerFnError::application(format!("Failed to apply Project manifest: {e}")))?;
	let deployment = Deployment::build()
		.organization(organization_id)
		.project_name(import_spec.project_name.clone())
		.cluster(cluster_id)
		.status("pending".to_string())
		.image(format!("{}:pending", import_spec.registry))
		.project_yaml(Some(manifest))
		.finish();
	let deployment = Deployment::objects()
		.create(&deployment)
		.await
		.map_err(|e| ServerFnError::application(format!("Failed to create deployment: {e}")))?;
	let deployment_id = deployment.id.ok_or_else(|| {
		ServerFnError::application("Deployment row missing primary key after insert")
	})?;
	let repository_row_id = repository
		.id
		.ok_or_else(|| ServerFnError::application("GitHub repository row missing primary key"))?;
	let production_branch = import_spec.branch.clone();
	let project = GitHubProject::build()
		.organization(organization_id)
		.repository(repository_row_id)
		.deployment(deployment_id)
		.project_name(import_spec.project_name)
		.production_branch(production_branch)
		.status("imported".to_string())
		.finish();
	let project = match GitHubProject::objects().create(&project).await {
		Ok(project) => project,
		Err(e) => {
			rollback_created_deployment(deployment_id).await;
			return Err(ServerFnError::application(format!(
				"Failed to create GitHub project: {e}"
			)));
		}
	};
	repository.selected = true;
	if let Err(e) = GitHubRepository::objects().update(&repository).await {
		if let Some(project_id) = project.id {
			rollback_created_github_project(project_id).await;
		}
		rollback_created_deployment(deployment_id).await;
		return Err(ServerFnError::application(format!(
			"Failed to update GitHub repository: {e}"
		)));
	}
	Ok(github_project_info(project))
}

#[server_fn]
pub async fn list_github_repositories_for_current_org(
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<Vec<GitHubRepositoryInfo>, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::github::models::{GitHubInstallation, GitHubRepository};

	ensure_github_account_linked(&user).await?;
	let organization_id = current_org_id(&user).await?;
	let installations = GitHubInstallation::objects()
		.filter(GitHubInstallation::field_organization_id().eq(organization_id))
		.filter(GitHubInstallation::field_status().eq("active"))
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
		let mut current = GitHubRepository::objects()
			.filter(GitHubRepository::field_installation_id().eq(row_id))
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

#[server_fn]
pub async fn list_github_project_previews_for_current_org(
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<Vec<ProjectPreviewSummary>, ServerFnError> {
	let user_id = user.id;

	#[cfg(native)]
	{
		use reinhardt::Model;

		use crate::apps::deployments::models::Deployment;
		use crate::apps::deployments::server_fn::ProjectSourceKind;
		use crate::apps::deployments::services::preview_status::{
			PreviewProjectInput, load_preview_summary,
		};
		use crate::apps::github::models::{GitHubProject, GitHubRepository};

		let organization_id =
			crate::apps::organizations::helpers::current_organization_id_for_user(user_id)
				.await
				.map_err(|e| ServerFnError::application(e.to_string()))?;
		let projects = GitHubProject::objects()
			.filter(GitHubProject::field_organization_id().eq(organization_id))
			.order_by(&["id"])
			.all()
			.await
			.map_err(|e| {
				ServerFnError::application(format!("Failed to list GitHub projects: {e}"))
			})?;

		let mut summaries = Vec::with_capacity(projects.len());
		for project in projects {
			let repository_id = *project.repository_id();
			let deployment_id = *project.deployment_id();
			let repository = GitHubRepository::objects()
				.filter(GitHubRepository::field_id().eq(repository_id))
				.first()
				.await
				.map_err(|e| {
					ServerFnError::application(format!("Failed to load GitHub repository: {e}"))
				})?
				.ok_or_else(|| ServerFnError::application("GitHub repository row is missing"))?;
			let deployment = Deployment::objects()
				.filter(Deployment::field_id().eq(deployment_id))
				.filter(Deployment::field_organization_id().eq(organization_id))
				.first()
				.await
				.map_err(|e| {
					ServerFnError::application(format!("Failed to load GitHub deployment: {e}"))
				})?
				.ok_or_else(|| ServerFnError::application("GitHub deployment row is missing"))?;
			let input = PreviewProjectInput {
				deployment_id,
				github_project_id: project.id,
				project_name: project.project_name,
				display_name: repository.full_name,
				production_branch: Some(project.production_branch),
				source_kind: ProjectSourceKind::GitHub,
				project_yaml: deployment.project_yaml,
			};
			summaries.push(load_preview_summary(input, "default").await);
		}
		Ok(summaries)
	}
	#[cfg(wasm)]
	{
		let _ = user_id;
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn list_github_repositories_for_installation(
	installation_row_id: i64,
	#[inject] reinhardt::CurrentUser(user): reinhardt::CurrentUser<crate::apps::auth::models::User>,
) -> Result<Vec<GitHubRepositoryInfo>, ServerFnError> {
	use reinhardt::Model;

	use crate::apps::github::models::{GitHubInstallation, GitHubRepository};

	ensure_github_account_linked(&user).await?;
	let organization_id = current_org_id(&user).await?;
	let installation = GitHubInstallation::objects()
		.filter(GitHubInstallation::field_id().eq(installation_row_id))
		.filter(GitHubInstallation::field_organization_id().eq(organization_id))
		.first()
		.await
		.map_err(|e| {
			ServerFnError::application(format!("Failed to load GitHub installation: {e}"))
		})?
		.ok_or_else(|| ServerFnError::server(404, "GitHub installation not found"))?;
	let row_id = installation
		.id
		.ok_or_else(|| ServerFnError::application("GitHub installation row missing primary key"))?;
	sync_repositories_for_installation(&installation).await?;
	let repositories = GitHubRepository::objects()
		.filter(GitHubRepository::field_installation_id().eq(row_id))
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
