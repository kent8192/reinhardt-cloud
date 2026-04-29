variable "name_prefix" {
  description = "Scoped prefix: <bootstrap_prefix>-<app_name>."
  type        = string
}

variable "project_id" {
  description = "GCP project ID."
  type        = string
}

variable "region" {
  description = "GCP region for the Cloud SQL instance."
  type        = string
}

variable "network_id" {
  description = "Self-link of the VPC network for private IP connectivity."
  type        = string
}

variable "database_version" {
  description = "Cloud SQL database version (e.g., POSTGRES_16)."
  type        = string
  default     = "POSTGRES_16"
}

variable "tier" {
  description = "Cloud SQL machine tier."
  type        = string
  default     = "db-f1-micro"
}

variable "backup_retention_days" {
  description = "Number of days to retain automated backups."
  type        = number
  default     = 7
}

variable "labels" {
  description = "Labels applied to the Cloud SQL instance."
  type        = map(string)
  default     = {}
}
