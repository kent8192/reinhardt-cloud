# GitHub Actions secrets and variables for the reinhardt-cloud repository.
# Secrets are encrypted at rest and masked in logs.
# Variables are plaintext and visible in workflow logs.

# --- Actions Secrets ---

resource "github_actions_secret" "release_plz_app_id" {
  repository      = var.repository_name
  secret_name     = "RELEASE_PLZ_APP_ID"
  plaintext_value = var.release_plz_app_id
}

resource "github_actions_secret" "release_plz_app_private_key" {
  repository      = var.repository_name
  secret_name     = "RELEASE_PLZ_APP_PRIVATE_KEY"
  plaintext_value = var.release_plz_app_private_key
}

resource "github_actions_secret" "dockerhub_username" {
  repository      = var.repository_name
  secret_name     = "DOCKERHUB_USERNAME"
  plaintext_value = var.dockerhub_username
}

resource "github_actions_secret" "dockerhub_token" {
  repository      = var.repository_name
  secret_name     = "DOCKERHUB_TOKEN"
  plaintext_value = var.dockerhub_token
}

resource "github_actions_secret" "codecov_token" {
  repository      = var.repository_name
  secret_name     = "CODECOV_TOKEN"
  plaintext_value = var.codecov_token
}

# --- Actions Variables ---

resource "github_actions_variable" "self_hosted_enabled" {
  repository    = var.repository_name
  variable_name = "SELF_HOSTED_ENABLED"
  value         = var.self_hosted_enabled
}
