variable "name_prefix" {
  description = "Prefix applied to all resource names for environment isolation."
  type        = string
}

variable "project_id" {
  description = "GCP project ID where the Cloud SQL instance is created."
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
  description = "Cloud SQL machine tier (e.g., db-f1-micro, db-custom-2-4096)."
  type        = string
  default     = "db-f1-micro"
}

variable "disk_size_gb" {
  description = "Initial disk size in GiB. Minimum 10 GiB for GCP."
  type        = number
  default     = 10
}

variable "availability_type" {
  description = "Availability type: ZONAL or REGIONAL."
  type        = string
  default     = "ZONAL"
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
