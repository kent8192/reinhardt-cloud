variable "name_prefix" {
  description = "Prefix applied to all resource names for environment isolation."
  type        = string
}

variable "vpc_id" {
  description = "VPC ID where the EKS cluster is deployed."
  type        = string
}

variable "subnet_ids" {
  description = "Private subnet IDs for EKS nodes."
  type        = list(string)
}

variable "kubernetes_version" {
  description = "Kubernetes version for the EKS cluster."
  type        = string
  default     = "1.31"
}

variable "node_instance_type" {
  description = "EC2 instance type for the managed node group."
  type        = string
  default     = "t3.medium"
}

variable "node_desired_count" {
  description = "Desired number of nodes in the managed node group."
  type        = number
  default     = 1
}

variable "node_min_count" {
  description = "Minimum number of nodes in the managed node group."
  type        = number
  default     = 1
}

variable "node_max_count" {
  description = "Maximum number of nodes in the managed node group."
  type        = number
  default     = 3
}

variable "tags" {
  description = "Tags applied to all EKS resources."
  type        = map(string)
  default     = {}
}
