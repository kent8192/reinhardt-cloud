variable "name_prefix" {
  description = "Prefix applied to all resource names for environment isolation."
  type        = string
}

variable "oidc_issuer_url" {
  description = "OIDC issuer URL from the EKS cluster (without the https:// prefix is added automatically)."
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

variable "rds_instance_id" {
  description = "RDS instance identifier to grant RDS access via IAM."
  type        = string
}

variable "tags" {
  description = "Tags applied to all IRSA resources."
  type        = map(string)
  default     = {}
}
