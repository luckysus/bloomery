use super::loader::DomainError;
use super::signature::{compute_package_digest, SIGNATURE_FILE};
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;
use std::fs;
use std::path::Path;
use uuid::Uuid;

/// Write the official Ed25519 signature envelope for a validated package root.
///
/// The private key is supplied by the release environment and never persisted
/// by this function. `signature.json` is excluded from the package digest, so
/// the digest can be calculated before the envelope is atomically installed.
pub fn sign_domain_package(
    root: &Path,
    signing_key: &SigningKey,
    key_id: &str,
) -> Result<(), DomainError> {
    if !root.is_dir() {
        return Err(DomainError::Io(
            "package root is not a directory".to_string(),
        ));
    }
    if key_id.trim().is_empty() || key_id != key_id.trim() {
        return Err(DomainError::Signature(
            "signing key id must not be empty or padded".to_string(),
        ));
    }

    let signature_path = root.join(SIGNATURE_FILE);
    if signature_path.exists() {
        return Err(DomainError::Signature(
            "signature file already exists".to_string(),
        ));
    }

    let package_sha256 = compute_package_digest(root)?;
    let signature = signing_key.sign(package_sha256.as_bytes());
    let envelope = json!({
        "key_id": key_id,
        "algorithm": "ed25519",
        "package_sha256": package_sha256,
        "signature": hex_bytes(&signature.to_bytes()),
    });
    let bytes = serde_json::to_vec_pretty(&envelope)
        .map_err(|error| DomainError::Signature(error.to_string()))?;
    let temporary_path = root.join(format!(".signature-{}.tmp", Uuid::new_v4()));

    if let Err(error) = fs::write(&temporary_path, bytes) {
        let _ = fs::remove_file(&temporary_path);
        return Err(DomainError::Io(error.to_string()));
    }
    if let Err(error) = fs::rename(&temporary_path, &signature_path) {
        let _ = fs::remove_file(&temporary_path);
        return Err(DomainError::Io(error.to_string()));
    }
    Ok(())
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
