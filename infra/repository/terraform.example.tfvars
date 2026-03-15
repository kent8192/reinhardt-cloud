# Copy this file to terraform.tfvars and fill in the actual values.
# terraform.tfvars is gitignored to prevent secret leakage.
#
# Usage:
#   cp terraform.example.tfvars terraform.tfvars
#   # Edit terraform.tfvars with actual values
#   terraform init
#   terraform plan
#   terraform apply

github_owner         = "kent8192"
github_token         = "ghp_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
repository_name      = "nuages"
environment_reviewer = "kent8192"

# GitHub App credentials for release-plz
# Create at: https://github.com/settings/apps
release_plz_app_id          = "000000"
release_plz_app_private_key = <<-EOT
  -----BEGIN RSA PRIVATE KEY-----
  (paste private key here)
  -----END RSA PRIVATE KEY-----
EOT

# Docker Hub credentials for integration test image pulls
# Create at: https://hub.docker.com/settings/security
dockerhub_username = "your-dockerhub-username"
dockerhub_token    = "dckr_pat_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"

# Codecov upload token
# Find at: https://app.codecov.io/gh/kent8192/nuages/settings
codecov_token = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"

# Self-hosted runner circuit breaker
# Set to "true" to enable self-hosted runners, "false" to force GitHub-hosted
self_hosted_enabled = "false"
