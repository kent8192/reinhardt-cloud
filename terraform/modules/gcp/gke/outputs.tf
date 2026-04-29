output "cluster_name" {
  description = "The name of the GKE cluster."
  value       = google_container_cluster.primary.name
}

output "cluster_endpoint" {
  description = "The endpoint for the GKE cluster API server."
  value       = google_container_cluster.primary.endpoint
  sensitive   = true
}

output "cluster_ca" {
  description = "The cluster CA certificate (base64-encoded) for kubeconfig."
  value       = google_container_cluster.primary.master_auth[0].cluster_ca_certificate
  sensitive   = true
}

output "node_sa_email" {
  description = "The email of the node pool service account."
  value       = google_service_account.nodes.email
}

output "workload_pool" {
  description = "The Workload Identity pool for this cluster."
  value       = "${var.project_id}.svc.id.goog"
}
