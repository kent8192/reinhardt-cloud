output "cluster_endpoint" {
  description = "GKE cluster API endpoint for kubeconfig."
  value       = module.gke.cluster_endpoint
  sensitive   = true
}

output "cluster_ca" {
  description = "GKE cluster CA certificate (base64) for kubeconfig."
  value       = module.gke.cluster_ca
  sensitive   = true
}

output "cloudsql_connection_name" {
  description = "Cloud SQL connection name for the dashboard DATABASE_URL."
  value       = module.cloudsql.connection_name
}

output "artifact_registry_url" {
  description = "Artifact Registry URL for container image push/pull."
  value       = module.artifact_registry.repository_url
}

output "operator_sa_email" {
  description = "Operator GSA email for Workload Identity annotation."
  value       = module.workload_identity.operator_sa_email
}

output "dashboard_sa_email" {
  description = "Dashboard GSA email for Workload Identity annotation."
  value       = module.workload_identity.dashboard_sa_email
}
