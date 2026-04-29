variable "name_prefix" {
  description = "Scoped prefix: <bootstrap_prefix>-<app_name>."
  type        = string
}

variable "project_id" {
  description = "GCP project ID."
  type        = string
}

variable "host" {
  description = "Fully-qualified DNS hostname (e.g., orders.acme.example.com)."
  type        = string
}

variable "record_type" {
  description = "DNS record type: A, CNAME, or TXT."
  type        = string
}

variable "managed_zone" {
  description = "Cloud DNS managed zone name."
  type        = string
}

variable "ttl" {
  description = "TTL in seconds for the DNS record."
  type        = number
  default     = 300
}

variable "rrdatas" {
  description = "Resource record data (e.g., IP addresses or CNAME targets)."
  type        = list(string)
  default     = []
}
