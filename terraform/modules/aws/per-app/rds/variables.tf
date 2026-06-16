variable "name_prefix" {
  description = "Scoped prefix: <bootstrap_prefix>-<project_name>."
  type        = string
}

variable "vpc_id" {
  description = "VPC ID for the RDS security group."
  type        = string
}

variable "subnet_ids" {
  description = "Private subnet IDs for the RDS subnet group."
  type        = list(string)
}

variable "allowed_security_group_ids" {
  description = "Security group IDs allowed to connect to the RDS instance."
  type        = list(string)
  default     = []
}

variable "instance_class" {
  description = "RDS instance class (e.g., db.t3.micro)."
  type        = string
  default     = "db.t3.micro"
}

variable "engine_version" {
  description = "PostgreSQL engine version (e.g., 16.3)."
  type        = string
  default     = "16.3"
}

variable "backup_retention_days" {
  description = "Number of days to retain automated backups."
  type        = number
  default     = 7
}

variable "db_password" {
  description = "Master password for the RDS instance. Never hardcode; pass via TF_VAR or Secrets Manager."
  type        = string
  sensitive   = true
}

variable "tags" {
  description = "Tags applied to all RDS resources."
  type        = map(string)
  default     = {}
}
