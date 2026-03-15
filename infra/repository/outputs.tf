output "fork_review_environment" {
	description = "Name of the fork review environment."
	value       = github_repository_environment.fork_review.environment
}

output "production_environment" {
	description = "Name of the production environment."
	value       = github_repository_environment.production.environment
}

output "managed_secrets" {
	description = "Names of GitHub Actions secrets managed by Terraform."
	value = [
		github_actions_secret.release_plz_app_id.secret_name,
		github_actions_secret.release_plz_app_private_key.secret_name,
		github_actions_secret.dockerhub_username.secret_name,
		github_actions_secret.dockerhub_token.secret_name,
		github_actions_secret.codecov_token.secret_name,
	]
}

output "managed_variables" {
	description = "Names of GitHub Actions variables managed by Terraform."
	value = [
		github_actions_variable.self_hosted_enabled.variable_name,
	]
}
