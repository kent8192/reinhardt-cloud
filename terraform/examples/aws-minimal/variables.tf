variable "region" {
  description = "AWS region (e.g., us-east-1)."
  type        = string
  default     = "us-east-1"
}

variable "name_prefix" {
  description = "Prefix for all resource names (e.g., reinhardt-dev)."
  type        = string
  default     = "reinhardt"
}

variable "availability_zones" {
  description = "Availability zones to deploy into."
  type        = list(string)
  default     = ["us-east-1a", "us-east-1b"]
}

variable "db_password" {
  description = "Master password for the RDS instance. Supply via TF_VAR_db_password or a secrets manager integration."
  type        = string
  sensitive   = true
}

variable "tags" {
  description = "Tags applied to all resources."
  type        = map(string)
  default = {
    ManagedBy   = "terraform"
    Environment = "dev"
  }
}
