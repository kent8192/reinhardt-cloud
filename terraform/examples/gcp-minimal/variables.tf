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

variable "enable_dataplane_v2" {
  description = "Enable GKE Dataplane V2 for NetworkPolicy enforcement in the example cluster."
  type        = bool
  default     = true
}

variable "labels" {
  description = "Labels applied to all resources."
  type        = map(string)
  default = {
    managed-by  = "terraform"
    environment = "dev"
  }
}
