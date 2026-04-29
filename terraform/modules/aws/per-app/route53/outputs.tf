output "fqdn" {
  description = "The fully-qualified domain name of the Route 53 record."
  value       = aws_route53_record.app.fqdn
}
