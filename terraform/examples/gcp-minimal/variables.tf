variable "project_id" {
  description = "GCP project ID."
  type        = string
}

variable "region" {
  description = "GCP region (e.g., us-central1)."
  type        = string
  default     = "us-central1"
}

variable "zone" {
  description = "GCP zone for the GKE cluster (e.g., us-central1-a)."
  type        = string
  default     = "us-central1-a"
}

variable "name_prefix" {
  description = "Prefix for all resource names (e.g., reinhardt-dev)."
  type        = string
  default     = "reinhardt"
}

variable "labels" {
  description = "Labels applied to all resources."
  type        = map(string)
  default = {
    managed-by  = "terraform"
    environment = "dev"
  }
}
