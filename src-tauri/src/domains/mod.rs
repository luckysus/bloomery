mod installer;
mod loader;
mod manifest;
mod signature;
mod trust;

pub use installer::{activate_package, cleanup_staging, install_package, InstalledDomainPackage};
pub use loader::{
    load_package, resolve_resource_path, DomainError, LoadedDomainPackage, ResolvedAsset,
};
pub use manifest::{
    AppCompatibility, AssetSpec, DataMapping, DomainManifest, EvaluationSpec, McpRecommendation,
    PromptSpec, RetrievalPolicy,
};
pub use signature::{compute_package_digest, DomainTrust, DomainTrustStore};
pub use trust::official_trust_store;
