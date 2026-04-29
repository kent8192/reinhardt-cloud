variable "name_prefix" {
  description = "Scoped prefix: <bootstrap_prefix>-<app_name>."
  type        = string
}

variable "project_id" {
  description = "GCP project ID."
  type        = string
}

variable "secret_name" {
  description = "Logical secret name; combined with name_prefix to form the Secret Manager secret ID."
  type        = string
}

variable "description" {
  description = "Human-readable description stored as secret metadata."
  type        = string
  default     = "Application secret"
}

variable "labels" {
  description = "Labels applied to the secret."
  type        = map(string)
  default     = {}
}
