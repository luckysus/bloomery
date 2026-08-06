use super::loader::DomainError;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

pub(crate) const SIGNATURE_FILE: &str = "signature.json";
const MAX_SIGNATURE_BYTES: usize = 16 * 1024;
const MAX_DIGEST_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DomainTrust {
    OfficialSigned,
    ThirdPartyUnsigned,
}

#[derive(Debug, Clone, Default)]
pub struct DomainTrustStore {
    official_keys: BTreeMap<String, VerifyingKey>,
}

impl DomainTrustStore {
    pub fn add_official_key(&mut self, key_id: impl Into<String>, key: VerifyingKey) {
        self.official_keys.insert(key_id.into(), key);
    }

    pub(crate) fn key(&self, key_id: &str) -> Option<&VerifyingKey> {
        self.official_keys.get(key_id)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignatureEnvelope {
    key_id: String,
    algorithm: String,
    package_sha256: String,
    signature: String,
}

pub fn compute_package_digest(root: &Path) -> Result<String, DomainError> {
    let mut files = Vec::new();
    collect_files(root, root, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut digest = Sha256::new();
    let mut total_bytes = 0_u64;
    for (relative, path) in files {
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| DomainError::Io(error.to_string()))?;
        let size = metadata.len();
        total_bytes = total_bytes.checked_add(size).ok_or_else(|| {
            DomainError::ResourceLimit("package digest size overflow".to_string())
        })?;
        if total_bytes > MAX_DIGEST_BYTES {
            return Err(DomainError::ResourceLimit(
                "package is too large to digest".to_string(),
            ));
        }
        let normalized = relative.to_string_lossy().replace('\\', "/");
        digest.update((normalized.len() as u64).to_le_bytes());
        digest.update(normalized.as_bytes());
        digest.update(size.to_le_bytes());
        let mut file = File::open(&path).map_err(|error| DomainError::Io(error.to_string()))?;
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let read = file
                .read(&mut buffer)
                .map_err(|error| DomainError::Io(error.to_string()))?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(crate) fn verify_package_signature(
    root: &Path,
    package_sha256: &str,
    trust_store: &DomainTrustStore,
) -> Result<DomainTrust, DomainError> {
    let path = root.join(SIGNATURE_FILE);
    if !path.exists() {
        return Ok(DomainTrust::ThirdPartyUnsigned);
    }
    let bytes = fs::read(&path).map_err(|error| DomainError::Io(error.to_string()))?;
    if bytes.len() > MAX_SIGNATURE_BYTES {
        return Err(DomainError::ResourceLimit(
            "signature file is too large".to_string(),
        ));
    }
    let envelope = serde_json::from_slice::<SignatureEnvelope>(&bytes)
        .map_err(|error| DomainError::Signature(error.to_string()))?;
    if envelope.key_id.trim().is_empty() || envelope.algorithm != "ed25519" {
        return Err(DomainError::Signature(
            "unsupported signature metadata".to_string(),
        ));
    }
    if !envelope.package_sha256.eq_ignore_ascii_case(package_sha256) {
        return Err(DomainError::Signature(
            "package hash does not match signature".to_string(),
        ));
    }
    let signature_bytes = decode_hex(&envelope.signature)
        .ok_or_else(|| DomainError::Signature("signature must be 64 hex bytes".to_string()))?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| DomainError::Signature("signature length is invalid".to_string()))?;
    let key = trust_store.key(&envelope.key_id).ok_or_else(|| {
        DomainError::Signature(format!("signing key is not trusted: {}", envelope.key_id))
    })?;
    key.verify(package_sha256.as_bytes(), &signature)
        .map_err(|_| DomainError::Signature("signature verification failed".to_string()))?;
    Ok(DomainTrust::OfficialSigned)
}

fn collect_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(PathBuf, PathBuf)>,
) -> Result<(), DomainError> {
    let entries = fs::read_dir(directory).map_err(|error| DomainError::Io(error.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|error| DomainError::Io(error.to_string()))?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|error| DomainError::UnsafePath(error.to_string()))?
            .to_path_buf();
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| DomainError::Io(error.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(DomainError::UnsafePath(relative.display().to_string()));
        }
        if metadata.is_dir() {
            collect_files(root, &path, files)?;
        } else if metadata.is_file() && relative != Path::new(SIGNATURE_FILE) {
            files.push((relative, path));
        } else if !metadata.is_file() {
            return Err(DomainError::InvalidResource(
                "package contains a non-regular file".to_string(),
            ));
        }
    }
    Ok(())
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if value.len() != 128 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut output = Vec::with_capacity(64);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = hex_value(pair[0])?;
        let low = hex_value(pair[1])?;
        output.push((high << 4) | low);
    }
    Some(output)
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}
