output "secret_id" {
  description = "The Secret Manager secret ID."
  value       = google_secret_manager_secret.app.secret_id
}

output "secret_name" {
  description = "The full resource name of the secret (projects/*/secrets/*)."
  value       = google_secret_manager_secret.app.name
}
