terraform {
  required_version = ">= 1.7"

  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 7.30"
    }
  }

  # Uncomment to use GCS remote state backend.
  # backend "gcs" {
  #   bucket = "<your-terraform-state-bucket>"
  #   prefix = "reinhardt-cloud/gcp-minimal"
  # }
}

provider "google" {
  project = var.project_id
  region  = var.region
}
