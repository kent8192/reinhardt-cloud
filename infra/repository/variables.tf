variable "github_owner" {
  description = "GitHub organization or user that owns the repository."
  type        = string
  default     = "kent8192"
}

variable "github_token" {
  description = "GitHub personal access token with admin repository permissions."
  type        = string
  sensitive   = true
}

variable "repository_name" {
  description = "Name of the GitHub repository to manage."
  type        = string
  default     = "nuages"
}

variable "environment_reviewer" {
  description = "GitHub username for environment deployment approvals."
  type        = string
}

# --- GitHub Actions Secrets ---

variable "release_plz_app_id" {
  description = "GitHub App ID for release-plz automation."
  type        = string
  sensitive   = true
}

variable "release_plz_app_private_key" {
  description = "GitHub App private key (PEM) for release-plz automation."
  type        = string
  sensitive   = true
}

variable "dockerhub_username" {
  description = "Docker Hub username for image pull rate limit avoidance."
  type        = string
  sensitive   = true
}

variable "dockerhub_token" {
  description = "Docker Hub access token for image pull authentication."
  type        = string
  sensitive   = true
}

variable "codecov_token" {
  description = "Codecov upload token for coverage reporting."
  type        = string
  sensitive   = true
}

# --- GitHub Actions Variables ---

variable "self_hosted_enabled" {
  description = "Circuit breaker for self-hosted runners. Set to 'true' to enable, 'false' to force GitHub-hosted."
  type        = bool
  default     = false
}
