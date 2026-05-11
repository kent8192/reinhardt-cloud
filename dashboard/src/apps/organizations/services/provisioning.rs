//! OrganizationProvisioningService.
//!
//! K8s namespace provisioning belongs to sub-issue #416. This module is a
//! placeholder for #415 to keep the app structure intact.
//!
//! No `#[injectable_factory]` conversion (kent8192/reinhardt-cloud#599):
//! the module is an 8-line placeholder with no implementation. When
//! #416 wires in real K8s namespace provisioning, the resulting service
//! will likely take `kube::Client` and other DI-resolvable handles
//! through `#[injectable_factory]` at that point; doing it now would
//! create empty scaffolding around `unimplemented!()`.

#[allow(dead_code)] // Implemented in sub-issue #416
pub struct OrganizationProvisioningService;
