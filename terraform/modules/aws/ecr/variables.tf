variable "name_prefix" {
  description = "Prefix applied to all resource names for environment isolation."
  type        = string
}

variable "image_retention_count" {
  description = "Number of tagged images to retain per repository."
  type        = number
  default     = 30
}

variable "tags" {
  description = "Tags applied to the ECR repository."
  type        = map(string)
  default     = {}
}
