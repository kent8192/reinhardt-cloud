output "vpc_id" {
  description = "The ID of the VPC."
  value       = aws_vpc.main.id
}

output "private_subnet_ids" {
  description = "IDs of the private subnets for EKS nodes and RDS."
  value       = aws_subnet.private[*].id
}

output "public_subnet_ids" {
  description = "IDs of the public subnets for NAT gateways and public load balancers."
  value       = aws_subnet.public[*].id
}
