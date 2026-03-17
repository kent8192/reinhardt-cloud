variable "aws_region" {
	description = "AWS region for runner infrastructure"
	type        = string
	default     = "us-east-1"
}

variable "aws_account_id" {
	description = "AWS Account ID. Used by init.sh to auto-generate backend.tfvars with the correct S3 bucket name."
	type        = string
}

variable "github_app_id" {
	description = "GitHub App ID for runner registration token generation"
	type        = string
}

variable "github_app_installation_id" {
	description = "GitHub App installation ID (shown in GitHub App settings after installation)"
	type        = string
}

variable "github_app_key_base64" {
	description = "GitHub App private key encoded in base64 (cat key.pem | base64 | tr -d newline)"
	type        = string
	sensitive   = true
}

variable "github_owner" {
	description = "GitHub repository owner username"
	type        = string
	default     = "kent8192"
}

variable "github_repository" {
	description = "GitHub repository name (without owner prefix)"
	type        = string
	default     = "reinhardt-nuages"
}

variable "prefix" {
	description = "Prefix for all AWS resource names"
	type        = string
	default     = "nuages-ci"
}

variable "enable_cancel_runner" {
	description = "Enable the always-on cancel runner (t4g.nano) for event-driven cancel workflows"
	type        = bool
	default     = true
}

variable "cancel_runner_instance_type" {
	description = "EC2 instance type for the cancel runner (API-only jobs, minimal resources needed)"
	type        = string
	default     = "t4g.nano"
}
