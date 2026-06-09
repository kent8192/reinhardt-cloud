//! GitHub App API client service.

use async_trait::async_trait;
use chrono::{Duration, Utc};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::config::GitHubAppSettings;

const ACCEPT_HEADER: &str = "application/vnd.github+json";
const API_VERSION_HEADER: &str = "2022-11-28";
const USER_AGENT_HEADER: &str = "reinhardt-cloud-dashboard";
const REPOSITORIES_PER_PAGE: u8 = 100;
const JWT_LIFETIME_MINUTES: i64 = 10;
const JWT_CLOCK_SKEW_SECONDS: i64 = 60;

/// Repository data returned by a GitHub App installation listing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitHubInstallationRepository {
	pub owner_login: String,
	pub id: i64,
	pub full_name: String,
	pub name: String,
	pub private: bool,
	pub default_branch: String,
}

/// Installation access token response returned by GitHub.
#[derive(Clone, Eq, PartialEq)]
pub struct GitHubInstallationAccessToken {
	pub token: String,
	pub expires_at: String,
}

/// GitHub App installation visible to a user access token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GitHubUserInstallation {
	pub id: i64,
	pub account_id: i64,
	pub account_login: String,
	pub account_type: String,
}

impl std::fmt::Debug for GitHubInstallationAccessToken {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("GitHubInstallationAccessToken")
			.field("token", &"[redacted]")
			.field("expires_at", &self.expires_at)
			.finish()
	}
}

/// Error surface for GitHub App API calls.
#[derive(Debug, Error)]
pub enum GitHubAppClientError {
	#[error("invalid GitHub App client configuration: {0}")]
	Config(String),
	#[error("failed to generate GitHub App JWT: {0}")]
	Jwt(String),
	#[error("GitHub API request failed: {0}")]
	Http(String),
	#[error("GitHub API returned {status}: {body}")]
	Api {
		status: reqwest::StatusCode,
		body: String,
	},
}

/// Abstract GitHub App API client.
#[async_trait]
pub trait GitHubAppClient: Send + Sync {
	async fn list_repositories(
		&self,
		installation_id: i64,
	) -> Result<Vec<GitHubInstallationRepository>, GitHubAppClientError>;

	async fn list_user_installations(
		&self,
		user_access_token: &str,
	) -> Result<Vec<GitHubUserInstallation>, GitHubAppClientError>;
}

/// Reqwest-backed GitHub App API client.
pub struct ReqwestGitHubAppClient {
	settings: GitHubAppSettings,
	http_client: reqwest::Client,
}

impl ReqwestGitHubAppClient {
	pub fn new(settings: GitHubAppSettings) -> Self {
		Self::with_http_client(settings, reqwest::Client::new())
	}

	pub fn with_http_client(settings: GitHubAppSettings, http_client: reqwest::Client) -> Self {
		Self {
			settings,
			http_client,
		}
	}

	pub(crate) async fn create_installation_access_token(
		&self,
		installation_id: i64,
	) -> Result<GitHubInstallationAccessToken, GitHubAppClientError> {
		let jwt = generate_app_jwt(&self.settings)?;
		let url = installation_access_tokens_url(&self.settings.api_base_url, installation_id)?;

		let response = self
			.http_client
			.post(url)
			.bearer_auth(jwt)
			.github_json_headers()
			.send()
			.await
			.map_err(|err| GitHubAppClientError::Http(err.to_string()))?;

		parse_response::<GitHubInstallationAccessTokenResponse>(response)
			.await
			.map(Into::into)
	}

	pub(crate) async fn list_repositories_with_token(
		&self,
		installation_token: &str,
	) -> Result<Vec<GitHubInstallationRepository>, GitHubAppClientError> {
		let mut page = 1u32;
		let mut repositories = Vec::new();

		loop {
			let url = installation_repositories_url(&self.settings.api_base_url)?;
			let response = self
				.http_client
				.get(url)
				.bearer_auth(installation_token)
				.github_json_headers()
				.query(&[
					("per_page", REPOSITORIES_PER_PAGE.to_string()),
					("page", page.to_string()),
				])
				.send()
				.await
				.map_err(|err| GitHubAppClientError::Http(err.to_string()))?;
			let page_response =
				parse_response::<GitHubInstallationRepositoriesResponse>(response).await?;
			let total_count = page_response.total_count;
			let page_len = page_response.repositories.len();
			repositories.extend(
				page_response
					.repositories
					.into_iter()
					.map(GitHubInstallationRepository::from),
			);

			if repositories.len() >= total_count || page_len < usize::from(REPOSITORIES_PER_PAGE) {
				break;
			}
			page += 1;
		}

		Ok(repositories)
	}

	pub(crate) async fn list_user_installations_with_token(
		&self,
		user_access_token: &str,
	) -> Result<Vec<GitHubUserInstallation>, GitHubAppClientError> {
		let mut page = 1u32;
		let mut installations = Vec::new();

		loop {
			let url = user_installations_url(&self.settings.api_base_url)?;
			let response = self
				.http_client
				.get(url)
				.bearer_auth(user_access_token)
				.github_json_headers()
				.query(&[
					("per_page", REPOSITORIES_PER_PAGE.to_string()),
					("page", page.to_string()),
				])
				.send()
				.await
				.map_err(|err| GitHubAppClientError::Http(err.to_string()))?;
			let page_response = parse_response::<GitHubUserInstallationsResponse>(response).await?;
			let total_count = page_response.total_count;
			let page_len = page_response.installations.len();
			installations.extend(
				page_response
					.installations
					.into_iter()
					.map(GitHubUserInstallation::from),
			);

			if installations.len() >= total_count || page_len < usize::from(REPOSITORIES_PER_PAGE) {
				break;
			}
			page += 1;
		}

		Ok(installations)
	}
}

#[async_trait]
impl GitHubAppClient for ReqwestGitHubAppClient {
	async fn list_repositories(
		&self,
		installation_id: i64,
	) -> Result<Vec<GitHubInstallationRepository>, GitHubAppClientError> {
		let access_token = self
			.create_installation_access_token(installation_id)
			.await?;
		self.list_repositories_with_token(&access_token.token).await
	}

	async fn list_user_installations(
		&self,
		user_access_token: &str,
	) -> Result<Vec<GitHubUserInstallation>, GitHubAppClientError> {
		self.list_user_installations_with_token(user_access_token)
			.await
	}
}

trait GitHubRequestBuilderExt {
	fn github_json_headers(self) -> Self;
}

impl GitHubRequestBuilderExt for reqwest::RequestBuilder {
	fn github_json_headers(self) -> Self {
		self.header(reqwest::header::ACCEPT, ACCEPT_HEADER)
			.header(reqwest::header::USER_AGENT, USER_AGENT_HEADER)
			.header("X-GitHub-Api-Version", API_VERSION_HEADER)
	}
}

#[derive(Debug, Serialize)]
struct GitHubAppJwtClaims {
	iat: i64,
	exp: i64,
	iss: String,
}

fn generate_app_jwt(settings: &GitHubAppSettings) -> Result<String, GitHubAppClientError> {
	let now = Utc::now();
	let claims = GitHubAppJwtClaims {
		iat: (now - Duration::seconds(JWT_CLOCK_SKEW_SECONDS)).timestamp(),
		exp: (now + Duration::minutes(JWT_LIFETIME_MINUTES)).timestamp(),
		iss: settings.app_id.to_string(),
	};
	let key = EncodingKey::from_rsa_pem(settings.private_key_pem.as_bytes())
		.map_err(|err| GitHubAppClientError::Jwt(err.to_string()))?;
	jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &key)
		.map_err(|err| GitHubAppClientError::Jwt(err.to_string()))
}

async fn parse_response<T>(response: reqwest::Response) -> Result<T, GitHubAppClientError>
where
	T: for<'de> Deserialize<'de>,
{
	let status = response.status();
	if !status.is_success() {
		let body = response.text().await.unwrap_or_default();
		return Err(GitHubAppClientError::Api { status, body });
	}
	response
		.json::<T>()
		.await
		.map_err(|err| GitHubAppClientError::Http(err.to_string()))
}

pub(crate) fn installation_access_tokens_url(
	api_base_url: &str,
	installation_id: i64,
) -> Result<Url, GitHubAppClientError> {
	api_url(
		api_base_url,
		&[
			"app",
			"installations",
			&installation_id.to_string(),
			"access_tokens",
		],
	)
}

pub(crate) fn installation_repositories_url(
	api_base_url: &str,
) -> Result<Url, GitHubAppClientError> {
	api_url(api_base_url, &["installation", "repositories"])
}

pub(crate) fn user_installations_url(api_base_url: &str) -> Result<Url, GitHubAppClientError> {
	api_url(api_base_url, &["user", "installations"])
}

fn api_url(api_base_url: &str, path_segments: &[&str]) -> Result<Url, GitHubAppClientError> {
	let mut url = Url::parse(api_base_url)
		.map_err(|err| GitHubAppClientError::Config(format!("invalid api_base_url: {err}")))?;
	{
		let mut segments = url.path_segments_mut().map_err(|_| {
			GitHubAppClientError::Config("api_base_url cannot be a base".to_string())
		})?;
		segments.pop_if_empty();
		segments.extend(path_segments);
	}
	Ok(url)
}

#[derive(Debug, Deserialize)]
struct GitHubInstallationAccessTokenResponse {
	token: String,
	expires_at: String,
}

impl From<GitHubInstallationAccessTokenResponse> for GitHubInstallationAccessToken {
	fn from(response: GitHubInstallationAccessTokenResponse) -> Self {
		Self {
			token: response.token,
			expires_at: response.expires_at,
		}
	}
}

#[derive(Debug, Deserialize)]
struct GitHubInstallationRepositoriesResponse {
	total_count: usize,
	repositories: Vec<GitHubRepositoryResponse>,
}

#[derive(Debug, Deserialize)]
struct GitHubRepositoryResponse {
	id: i64,
	full_name: String,
	name: String,
	private: bool,
	default_branch: String,
	owner: GitHubRepositoryOwnerResponse,
}

#[derive(Debug, Deserialize)]
struct GitHubRepositoryOwnerResponse {
	login: String,
}

impl From<GitHubRepositoryResponse> for GitHubInstallationRepository {
	fn from(response: GitHubRepositoryResponse) -> Self {
		Self {
			owner_login: response.owner.login,
			id: response.id,
			full_name: response.full_name,
			name: response.name,
			private: response.private,
			default_branch: response.default_branch,
		}
	}
}

#[derive(Debug, Deserialize)]
struct GitHubUserInstallationsResponse {
	total_count: usize,
	installations: Vec<GitHubUserInstallationResponse>,
}

#[derive(Debug, Deserialize)]
struct GitHubUserInstallationResponse {
	id: i64,
	account: GitHubUserInstallationAccountResponse,
}

#[derive(Debug, Deserialize)]
struct GitHubUserInstallationAccountResponse {
	id: i64,
	login: String,
	#[serde(rename = "type")]
	account_type: String,
}

impl From<GitHubUserInstallationResponse> for GitHubUserInstallation {
	fn from(response: GitHubUserInstallationResponse) -> Self {
		Self {
			id: response.id,
			account_id: response.account.id,
			account_login: response.account.login,
			account_type: response.account.account_type,
		}
	}
}
