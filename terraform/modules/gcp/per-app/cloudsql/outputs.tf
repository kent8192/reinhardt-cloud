output "connection_name" {
  description = "Cloud SQL connection name (project:region:instance)."
  value       = google_sql_database_instance.app.connection_name
}

output "private_ip" {
  description = "Private IP address of the Cloud SQL instance."
  value       = google_sql_database_instance.app.private_ip_address
  sensitive   = true
}

output "database_name" {
  description = "The name of the application database."
  value       = google_sql_database.app.name
}
