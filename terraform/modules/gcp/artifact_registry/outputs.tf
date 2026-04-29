output "repository_url" {
  description = "The URL of the Artifact Registry repository (region-docker.pkg.dev/project/repo)."
  value       = "${var.region}-docker.pkg.dev/${var.project_id}/${google_artifact_registry_repository.images.repository_id}"
}

output "repository_name" {
  description = "The short name of the Artifact Registry repository."
  value       = google_artifact_registry_repository.images.repository_id
}
