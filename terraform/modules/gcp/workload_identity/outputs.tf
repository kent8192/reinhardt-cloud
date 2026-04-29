output "operator_sa_email" {
  description = "The email of the operator GSA annotated on the Kubernetes service account."
  value       = google_service_account.operator.email
}

output "dashboard_sa_email" {
  description = "The email of the dashboard GSA annotated on the Kubernetes service account."
  value       = google_service_account.dashboard.email
}
