variable "name_prefix" {
  description = "Scoped prefix: <bootstrap_prefix>-<project_name>."
  type        = string
}

variable "zone_id" {
  description = "Route 53 hosted zone ID."
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

variable "ttl" {
  description = "TTL in seconds."
  type        = number
  default     = 300
}

variable "records" {
  description = "List of record values (IP addresses, CNAME targets, or TXT strings)."
  type        = list(string)
  default     = []
}

variable "tags" {
  description = "Tags applied to the Route 53 record."
  type        = map(string)
  default     = {}
}
