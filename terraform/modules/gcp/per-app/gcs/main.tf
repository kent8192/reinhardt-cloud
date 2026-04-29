resource "google_storage_bucket" "app" {
  project                     = var.project_id
  name                        = "${var.name_prefix}-${var.bucket_name}"
  location                    = "US"
  force_destroy               = false
  uniform_bucket_level_access = true

  labels = var.labels
}

resource "google_storage_bucket_iam_member" "public_read" {
  count  = var.public ? 1 : 0
  bucket = google_storage_bucket.app.name
  role   = "roles/storage.objectViewer"
  member = "allUsers"
}
