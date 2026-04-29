resource "google_dns_record_set" "app" {
  project      = var.project_id
  name         = "${var.host}."
  type         = var.record_type
  ttl          = var.ttl
  managed_zone = var.managed_zone
  rrdatas      = var.rrdatas
}
