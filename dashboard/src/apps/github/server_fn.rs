//! GitHub App server functions for the WASM dashboard.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitHubInstallationInfo {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct GitHubRepositoryInfo {}

#[server_fn]
pub async fn list_github_installations_for_current_org()
-> Result<Vec<GitHubInstallationInfo>, ServerFnError> {
	#[cfg(native)]
	{
		Err(ServerFnError::server(
			501,
			"GitHub installation listing is not implemented",
		))
	}
	#[cfg(wasm)]
	{
		unreachable!("server_fn body is replaced on wasm")
	}
}

#[server_fn]
pub async fn list_github_repositories_for_installation(
	_installation_id: i64,
) -> Result<Vec<GitHubRepositoryInfo>, ServerFnError> {
	#[cfg(native)]
	{
		Err(ServerFnError::server(
			501,
			"GitHub repository listing is not implemented",
		))
	}
	#[cfg(wasm)]
	{
		unreachable!("server_fn body is replaced on wasm")
	}
}
