variable "name_prefix" {
  description = "Prefix applied to all resource names for environment isolation."
  type        = string
}

variable "project_id" {
  description = "GCP project ID where the Artifact Registry repository is created."
  type        = string
}

variable "region" {
  description = "GCP region for the Artifact Registry repository."
  type        = string
}

variable "labels" {
  description = "Labels applied to the Artifact Registry repository."
  type        = map(string)
  default     = {}
}
