output "endpoint" {
  description = "The RDS connection endpoint (host:port)."
  value       = aws_db_instance.app.endpoint
  sensitive   = true
}

output "db_name" {
  description = "The name of the application database."
  value       = aws_db_instance.app.db_name
}

output "security_group_id" {
  description = "The security group ID of the RDS instance."
  value       = aws_security_group.rds.id
}
