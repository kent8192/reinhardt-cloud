output "vpc_id" {
  description = "The self-link of the VPC network."
  value       = google_compute_network.vpc.id
}

output "vpc_name" {
  description = "The name of the VPC network."
  value       = google_compute_network.vpc.name
}

output "subnet_id" {
  description = "The self-link of the primary subnetwork."
  value       = google_compute_subnetwork.primary.id
}

output "subnet_name" {
  description = "The name of the primary subnetwork."
  value       = google_compute_subnetwork.primary.name
}

output "pods_range_name" {
  description = "The name of the secondary IP range for GKE pods."
  value       = "${var.name_prefix}-pods"
}

output "services_range_name" {
  description = "The name of the secondary IP range for GKE services."
  value       = "${var.name_prefix}-services"
}
