variable "name_prefix" {
  description = "Prefix applied to all resource names for environment isolation."
  type        = string
}

variable "project_id" {
  description = "GCP project ID where network resources are created."
  type        = string
}

variable "region" {
  description = "GCP region for the VPC and subnets."
  type        = string
}

variable "subnet_cidr" {
  description = "CIDR block for the primary subnet."
  type        = string
  default     = "10.0.0.0/20"
}

variable "pods_cidr" {
  description = "Secondary CIDR block for GKE pods."
  type        = string
  default     = "10.4.0.0/14"
}

variable "services_cidr" {
  description = "Secondary CIDR block for GKE services."
  type        = string
  default     = "10.8.0.0/20"
}

variable "labels" {
  description = "Labels applied to all network resources."
  type        = map(string)
  default     = {}
}
