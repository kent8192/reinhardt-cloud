output "endpoint" {
  description = "The connection endpoint for the RDS instance (host:port)."
  value       = aws_db_instance.primary.endpoint
  sensitive   = true
}

output "db_name" {
  description = "The name of the initial database."
  value       = aws_db_instance.primary.db_name
}

output "instance_id" {
  description = "The RDS instance identifier."
  value       = aws_db_instance.primary.identifier
}

output "security_group_id" {
  description = "The security group ID of the RDS instance."
  value       = aws_security_group.rds.id
}
