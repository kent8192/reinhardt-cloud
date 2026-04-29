variable "name_prefix" {
  description = "Prefix applied to all resource names for environment isolation."
  type        = string
}

variable "project_id" {
  description = "GCP project ID where the GKE cluster is created."
  type        = string
}

variable "region" {
  description = "GCP region for the GKE cluster."
  type        = string
}

variable "zone" {
  description = "GCP zone for the default node pool (used for zonal clusters)."
  type        = string
}

variable "network_id" {
  description = "Self-link of the VPC network for the cluster."
  type        = string
}

variable "subnet_id" {
  description = "Self-link of the subnetwork for the cluster nodes."
  type        = string
}

variable "pods_range_name" {
  description = "Name of the secondary IP range for pods."
  type        = string
}

variable "services_range_name" {
  description = "Name of the secondary IP range for services."
  type        = string
}

variable "node_count" {
  description = "Initial number of nodes in the default node pool."
  type        = number
  default     = 1
}

variable "machine_type" {
  description = "Machine type for the default node pool."
  type        = string
  default     = "e2-standard-2"
}

variable "disk_size_gb" {
  description = "Boot disk size in GiB for each node."
  type        = number
  default     = 50
}

variable "labels" {
  description = "Labels applied to all GKE resources."
  type        = map(string)
  default     = {}
}
