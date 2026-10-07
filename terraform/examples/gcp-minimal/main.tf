module "network" {
  source      = "../../modules/gcp/network"
  name_prefix = var.name_prefix
  project_id  = var.project_id
  region      = var.region
  labels      = var.labels
}

module "gke" {
  source              = "../../modules/gcp/gke"
  name_prefix         = var.name_prefix
  project_id          = var.project_id
  region              = var.region
  zone                = var.zone
  network_id          = module.network.vpc_id
  subnet_id           = module.network.subnet_id
  pods_range_name     = module.network.pods_range_name
  services_range_name = module.network.services_range_name
  enable_dataplane_v2 = var.enable_dataplane_v2
  labels              = var.labels
}

module "cloudsql" {
  source      = "../../modules/gcp/cloudsql"
  name_prefix = var.name_prefix
  project_id  = var.project_id
  region      = var.region
  network_id  = module.network.vpc_id
  labels      = var.labels
}

module "artifact_registry" {
  source      = "../../modules/gcp/artifact_registry"
  name_prefix = var.name_prefix
  project_id  = var.project_id
  region      = var.region
  labels      = var.labels
}

module "workload_identity" {
  source                 = "../../modules/gcp/workload_identity"
  name_prefix            = var.name_prefix
  project_id             = var.project_id
  cluster_project_id     = var.project_id
  cloudsql_instance_name = module.cloudsql.instance_name
}
