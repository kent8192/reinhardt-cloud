resource "google_sql_database_instance" "primary" {
  project          = var.project_id
  name             = "${var.name_prefix}-db"
  region           = var.region
  database_version = var.database_version

  settings {
    tier              = var.tier
    availability_type = var.availability_type
    disk_size         = var.disk_size_gb
    disk_autoresize   = true

    backup_configuration {
      enabled                        = true
      point_in_time_recovery_enabled = var.availability_type == "REGIONAL"
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

resource "google_sql_database" "reinhardt" {
  project  = var.project_id
  name     = "reinhardt"
  instance = google_sql_database_instance.primary.name
}

resource "google_sql_user" "operator" {
  project  = var.project_id
  name     = "reinhardt-operator"
  instance = google_sql_database_instance.primary.name
  type     = "CLOUD_IAM_SERVICE_ACCOUNT"
}
