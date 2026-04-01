output "cancel_runner_instance_id" {
	description = "EC2 instance ID of the cancel runner (empty if disabled)"
	value       = var.enable_cancel_runner ? aws_instance.cancel_runner[0].id : ""
}

output "cancel_runner_private_ip" {
	description = "Private IP of the cancel runner (for SSM Session Manager access)"
	value       = var.enable_cancel_runner ? aws_instance.cancel_runner[0].private_ip : ""
}
