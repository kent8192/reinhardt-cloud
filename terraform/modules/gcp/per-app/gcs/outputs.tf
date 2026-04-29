output "bucket_name" {
  description = "The full name of the GCS bucket."
  value       = google_storage_bucket.app.name
}

output "bucket_url" {
  description = "The gs:// URL of the bucket."
  value       = google_storage_bucket.app.url
}
