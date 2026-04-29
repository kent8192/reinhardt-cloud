variable "name_prefix" {
  description = "Scoped prefix: <bootstrap_prefix>-<app_name>."
  type        = string
}

variable "bucket_name" {
  description = "Logical bucket name; combined with name_prefix to form the S3 bucket name."
  type        = string
}

variable "public" {
  description = "When true, allow public read access (e.g., for static assets). Default is false."
  type        = bool
  default     = false
}

variable "tags" {
  description = "Tags applied to the S3 bucket."
  type        = map(string)
  default     = {}
}
