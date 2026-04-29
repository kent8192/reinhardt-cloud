output "fqdn" {
  description = "The fully-qualified domain name of the DNS record."
  value       = google_dns_record_set.app.name
}
