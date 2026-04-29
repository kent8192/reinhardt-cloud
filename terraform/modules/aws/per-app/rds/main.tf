resource "aws_db_subnet_group" "app" {
  name       = "${var.name_prefix}-db-subnet-group"
  subnet_ids = var.subnet_ids

  tags = merge(var.tags, { Name = "${var.name_prefix}-db-subnet-group" })
}

resource "aws_security_group" "rds" {
  name        = "${var.name_prefix}-rds-sg"
  description = "Security group for the per-app RDS instance."
  vpc_id      = var.vpc_id

  ingress {
    from_port       = 5432
    to_port         = 5432
    protocol        = "tcp"
    security_groups = var.allowed_security_group_ids
    description     = "Allow Postgres from EKS nodes."
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
    description = "Allow all outbound traffic."
  }

  tags = merge(var.tags, { Name = "${var.name_prefix}-rds-sg" })
}

resource "aws_db_instance" "app" {
  identifier        = "${var.name_prefix}-db"
  engine            = "postgres"
  engine_version    = var.engine_version
  instance_class    = var.instance_class
  allocated_storage = 20
  storage_encrypted = true

  db_name  = replace(var.name_prefix, "-", "_")
  username = "app_admin"
  password = var.db_password

  db_subnet_group_name   = aws_db_subnet_group.app.name
  vpc_security_group_ids = [aws_security_group.rds.id]

  backup_retention_period   = var.backup_retention_days
  skip_final_snapshot       = false
  final_snapshot_identifier = "${var.name_prefix}-db-final-snapshot"

  # Instances connect through private VPC network only.
  publicly_accessible = false

  # Enable CloudWatch logs for audit and troubleshooting.
  enabled_cloudwatch_logs_exports = ["postgresql", "upgrade"]

  tags = var.tags
}
