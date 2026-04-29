variable "project_id" {
  description = "GCP project ID where service accounts are created."
  type        = string
}

variable "name_prefix" {
  description = "Prefix applied to all resource names for environment isolation."
  type        = string
}

variable "cluster_project_id" {
  description = "GCP project ID of the GKE cluster (may differ from service account project)."
  type        = string
}

variable "operator_namespace" {
  description = "Kubernetes namespace of the reinhardt-cloud operator."
  type        = string
  default     = "reinhardt-cloud-system"
}

variable "operator_ksa_name" {
  description = "Kubernetes service account name for the operator."
  type        = string
  default     = "reinhardt-cloud-operator"
}

variable "dashboard_namespace" {
  description = "Kubernetes namespace of the reinhardt-web dashboard."
  type        = string
  default     = "reinhardt-cloud"
}

variable "dashboard_ksa_name" {
  description = "Kubernetes service account name for the dashboard."
  type        = string
  default     = "reinhardt-cloud-dashboard"
}

variable "cloudsql_instance_name" {
  description = "Cloud SQL instance name to grant access to."
  type        = string
}
