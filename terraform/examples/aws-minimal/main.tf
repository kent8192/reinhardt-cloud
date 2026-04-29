module "network" {
  source             = "../../modules/aws/network"
  name_prefix        = var.name_prefix
  availability_zones = var.availability_zones
  tags               = var.tags
}

module "eks" {
  source      = "../../modules/aws/eks"
  name_prefix = var.name_prefix
  vpc_id      = module.network.vpc_id
  subnet_ids  = module.network.private_subnet_ids
  tags        = var.tags
}

module "rds" {
  source                     = "../../modules/aws/rds"
  name_prefix                = var.name_prefix
  vpc_id                     = module.network.vpc_id
  subnet_ids                 = module.network.private_subnet_ids
  allowed_security_group_ids = []
  db_password                = var.db_password
  tags                       = var.tags
}

module "ecr" {
  source      = "../../modules/aws/ecr"
  name_prefix = var.name_prefix
  tags        = var.tags
}

module "irsa" {
  source          = "../../modules/aws/irsa"
  name_prefix     = var.name_prefix
  oidc_issuer_url = module.eks.oidc_issuer_url
  rds_instance_id = module.rds.instance_id
  tags            = var.tags
}
