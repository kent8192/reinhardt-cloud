output "connection_name" {
  description = "The Cloud SQL connection name used by Cloud SQL Auth Proxy (project:region:instance)."
  value       = google_sql_database_instance.primary.connection_name
}

output "private_ip" {
  description = "The private IP address of the Cloud SQL instance."
  value       = google_sql_database_instance.primary.private_ip_address
  sensitive   = true
}

output "database_name" {
  description = "The name of the reinhardt database."
  value       = google_sql_database.reinhardt.name
}

output "instance_name" {
  description = "The name of the Cloud SQL instance."
  value       = google_sql_database_instance.primary.name
}
