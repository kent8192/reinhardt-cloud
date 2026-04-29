resource "google_artifact_registry_repository" "images" {
  project       = var.project_id
  location      = var.region
  repository_id = "${var.name_prefix}-images"
  description   = "Reinhardt Cloud container images"
  format        = "DOCKER"

  labels = var.labels
}

resource "google_artifact_registry_repository_iam_member" "operator_reader" {
  project    = var.project_id
  location   = var.region
  repository = google_artifact_registry_repository.images.name
  role       = "roles/artifactregistry.reader"
  member     = "serviceAccount:${var.project_id}@appspot.gserviceaccount.com"
}
