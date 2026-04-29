variable "name_prefix" {
  description = "Prefix applied to all resource names for environment isolation."
  type        = string
}

variable "vpc_id" {
  description = "VPC ID where the RDS instance is deployed."
  type        = string
}

variable "subnet_ids" {
  description = "Private subnet IDs for the RDS subnet group."
  type        = list(string)
}

variable "allowed_security_group_ids" {
  description = "Security group IDs allowed to connect to RDS (e.g., EKS node group SG)."
  type        = list(string)
  default     = []
}

variable "engine_version" {
  description = "PostgreSQL engine version."
  type        = string
  default     = "16.3"
}

variable "instance_class" {
  description = "RDS instance class (e.g., db.t3.micro)."
  type        = string
  default     = "db.t3.micro"
}

variable "allocated_storage_gb" {
  description = "Initial allocated storage in GiB."
  type        = number
  default     = 20
}

variable "multi_az" {
  description = "Enable Multi-AZ deployment for high availability."
  type        = bool
  default     = false
}

variable "backup_retention_days" {
  description = "Number of days to retain automated backups."
  type        = number
  default     = 7
}

variable "db_name" {
  description = "Name of the initial database created in the RDS instance."
  type        = string
  default     = "reinhardt"
}

variable "db_username" {
  description = "Master username for the RDS instance."
  type        = string
  default     = "reinhardt_admin"
}

variable "db_password" {
  description = "Master password for the RDS instance. Store in AWS Secrets Manager; never hardcode."
  type        = string
  sensitive   = true
}

variable "tags" {
  description = "Tags applied to all RDS resources."
  type        = map(string)
  default     = {}
}
