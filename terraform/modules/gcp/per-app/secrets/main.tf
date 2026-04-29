resource "google_secret_manager_secret" "app" {
  project   = var.project_id
  secret_id = "${var.name_prefix}-${var.secret_name}"

  labels = var.labels

  replication {
    auto {}
  }
}
