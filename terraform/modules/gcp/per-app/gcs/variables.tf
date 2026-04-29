variable "name_prefix" {
  description = "Scoped prefix: <bootstrap_prefix>-<app_name>."
  type        = string
}

variable "project_id" {
  description = "GCP project ID."
  type        = string
}

variable "bucket_name" {
  description = "Logical bucket name; combined with name_prefix to form the full bucket name."
  type        = string
}

variable "public" {
  description = "When true, allow public read access (e.g., for static assets)."
  type        = bool
  default     = false
}

variable "labels" {
  description = "Labels applied to the bucket."
  type        = map(string)
  default     = {}
}
