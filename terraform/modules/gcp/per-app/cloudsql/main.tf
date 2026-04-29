resource "google_sql_database_instance" "app" {
  project          = var.project_id
  name             = "${var.name_prefix}-db"
  region           = var.region
  database_version = var.database_version

  settings {
    tier              = var.tier
    availability_type = "ZONAL"
    disk_autoresize   = true

    backup_configuration {
      enabled = true
      backup_retention_settings {
        retained_backups = var.backup_retention_days
      }
    }

    ip_configuration {
      ipv4_enabled                                  = false
      private_network                               = var.network_id
      enable_private_path_for_google_cloud_services = true
    }

    database_flags {
      name  = "cloudsql.iam_authentication"
      value = "on"
    }

    user_labels = var.labels
  }

  deletion_protection = false
}

resource "google_sql_database" "app" {
  project  = var.project_id
  name     = replace(var.name_prefix, "-", "_")
  instance = google_sql_database_instance.app.name
}
