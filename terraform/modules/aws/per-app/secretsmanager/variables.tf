variable "name_prefix" {
  description = "Scoped prefix: <bootstrap_prefix>-<app_name>."
  type        = string
}

variable "secret_name" {
  description = "Logical secret name; combined with name_prefix to form the Secrets Manager secret name."
  type        = string
}

variable "description" {
  description = "Human-readable description stored as secret metadata."
  type        = string
  default     = "Application secret"
}

variable "kms_key_id" {
  description = "ARN or ID of the KMS key used to encrypt the secret. Defaults to the AWS-managed key when null."
  type        = string
  default     = null
}

variable "recovery_window_days" {
  description = "Number of days before a deleted secret can be permanently deleted (0 for immediate)."
  type        = number
  default     = 7
}

variable "tags" {
  description = "Tags applied to the secret."
  type        = map(string)
  default     = {}
}
