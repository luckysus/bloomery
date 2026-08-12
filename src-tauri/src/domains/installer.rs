use super::loader::{load_package, resolve_resource_path, DomainError, LoadedDomainPackage};
use super::signature::{
    compute_package_digest, verify_package_signature, DomainTrust, DomainTrustStore, SIGNATURE_FILE,
};
use crate::permissions::path::authorize_existing_path;
use serde::Serialize;
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;
use zip::ZipArchive;

const STAGING_DIR: &str = ".staging";
const MAX_PACKAGE_FILES: usize = 512;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct InstalledDomainPackage {
    pub path: PathBuf,
    pub manifest: super::manifest::DomainManifest,
    pub package_sha256: String,
    pub trust: DomainTrust,
}

pub fn install_package(
    source: &Path,
    install_root: &Path,
    app_version: &str,
    trust_store: &DomainTrustStore,
) -> Result<InstalledDomainPackage, DomainError> {
    let authorized_source = authorize_existing_path(source)
        .map_err(|error| DomainError::UnsafePath(error.to_string()))?;
    let source = authorized_source.canonical_path();
    fs::create_dir_all(install_root).map_err(|error| DomainError::Io(error.to_string()))?;
    let staging_root = install_root.join(STAGING_DIR);
    fs::create_dir_all(&staging_root).map_err(|error| DomainError::Io(error.to_string()))?;
    let staging = staging_root.join(Uuid::new_v4().to_string());
    fs::create_dir_all(&staging).map_err(|error| DomainError::Io(error.to_string()))?;

    let result = install_into_staging(source, &staging, install_root, app_version, trust_store);
    match result {
        Ok(package) => Ok(package),
        Err(error) => {
            let _ = fs::remove_dir_all(&staging);
            Err(error)
        }
    }
}

pub fn activate_package(
    install_root: &Path,
    package_id: &str,
    version: &str,
    app_version: &str,
) -> Result<LoadedDomainPackage, DomainError> {
    let package_path = package_path(install_root, package_id, version)?;
    load_package(&package_path, app_version)
}

pub fn cleanup_staging(install_root: &Path) -> Result<usize, DomainError> {
    let staging_root = install_root.join(STAGING_DIR);
    if !staging_root.exists() {
        return Ok(0);
    }
    if !staging_root.is_dir() {
        return Err(DomainError::InvalidResource(
            "domain staging path is not a directory".to_string(),
        ));
    }
    let mut removed = 0;
    for entry in fs::read_dir(&staging_root).map_err(|error| DomainError::Io(error.to_string()))? {
        let entry = entry.map_err(|error| DomainError::Io(error.to_string()))?;
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| DomainError::Io(error.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(DomainError::UnsafePath(path.display().to_string()));
        }
        fs::remove_dir_all(&path).map_err(|error| DomainError::Io(error.to_string()))?;
        removed += 1;
    }
    Ok(removed)
}

fn install_into_staging(
    source: &Path,
    staging: &Path,
    install_root: &Path,
    app_version: &str,
    trust_store: &DomainTrustStore,
) -> Result<InstalledDomainPackage, DomainError> {
    if source.is_dir() {
        copy_tree(source, staging)?;
    } else if source.extension().and_then(|extension| extension.to_str()) == Some("zip") {
        extract_archive(source, staging)?;
    } else {
        return Err(DomainError::InvalidResource(
            "package source must be a directory or .zip archive".to_string(),
        ));
    }
    let loaded = load_package(staging, app_version)?;
    validate_package_files(staging, &loaded.assets)?;
    let package_sha256 = compute_package_digest(staging)?;
    let trust = verify_package_signature(staging, &package_sha256, trust_store)?;
    let destination = package_path(install_root, &loaded.manifest.id, &loaded.manifest.version)?;
    if destination.exists() {
        return Err(DomainError::InvalidResource(format!(
            "package version is already installed: {}/{}",
            loaded.manifest.id, loaded.manifest.version
        )));
    }
    fs::create_dir_all(destination.parent().expect("package parent"))
        .map_err(|error| DomainError::Io(error.to_string()))?;
    fs::rename(staging, &destination).map_err(|error| DomainError::Io(error.to_string()))?;
    Ok(InstalledDomainPackage {
        path: destination,
        manifest: loaded.manifest,
        package_sha256,
        trust,
    })
}

fn package_path(root: &Path, package_id: &str, version: &str) -> Result<PathBuf, DomainError> {
    validate_path_component(package_id)?;
    validate_path_component(version)?;
    Ok(root.join(package_id).join(version))
}

fn validate_path_component(value: &str) -> Result<(), DomainError> {
    let path = Path::new(value);
    if value.is_empty()
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
    {
        return Err(DomainError::UnsafePath(value.to_string()));
    }
    Ok(())
}

fn validate_package_files(
    root: &Path,
    assets: &[super::loader::ResolvedAsset],
) -> Result<(), DomainError> {
    let mut allowed = HashSet::new();
    allowed.insert(PathBuf::from("manifest.json"));
    allowed.insert(PathBuf::from(SIGNATURE_FILE));
    allowed.extend(assets.iter().map(|asset| asset.relative_path.clone()));
    let mut files = Vec::new();
    collect_package_files(root, root, &mut files)?;
    if files.len() > MAX_PACKAGE_FILES {
        return Err(DomainError::ResourceLimit(
            "too many package files".to_string(),
        ));
    }
    let mut total_bytes = 0_u64;
    for relative in files {
        reject_executable_extension(&relative)?;
        if !allowed.contains(&relative) {
            return Err(DomainError::InvalidResource(format!(
                "file is not declared by manifest: {}",
                relative.display()
            )));
        }
        let size = fs::metadata(root.join(&relative))
            .map_err(|error| DomainError::Io(error.to_string()))?
            .len();
        if size > MAX_FILE_BYTES {
            return Err(DomainError::ResourceLimit(format!(
                "package file is too large: {}",
                relative.display()
            )));
        }
        total_bytes = total_bytes
            .checked_add(size)
            .ok_or_else(|| DomainError::ResourceLimit("package size overflow".to_string()))?;
        if total_bytes > MAX_TOTAL_BYTES {
            return Err(DomainError::ResourceLimit(
                "package is too large".to_string(),
            ));
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), DomainError> {
    for entry in fs::read_dir(source).map_err(|error| DomainError::Io(error.to_string()))? {
        let entry = entry.map_err(|error| DomainError::Io(error.to_string()))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)
            .map_err(|error| DomainError::Io(error.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(DomainError::UnsafePath(source_path.display().to_string()));
        }
        if metadata.is_dir() {
            fs::create_dir_all(&destination_path)
                .map_err(|error| DomainError::Io(error.to_string()))?;
            copy_tree(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &destination_path)
                .map_err(|error| DomainError::Io(error.to_string()))?;
        } else {
            return Err(DomainError::InvalidResource(
                "package contains a non-regular file".to_string(),
            ));
        }
    }
    Ok(())
}

fn extract_archive(source: &Path, destination: &Path) -> Result<(), DomainError> {
    let file = File::open(source).map_err(|error| DomainError::Io(error.to_string()))?;
    let mut archive = ZipArchive::new(file).map_err(|error| {
        DomainError::InvalidResource(format!("invalid package archive: {error}"))
    })?;
    if archive.len() > MAX_PACKAGE_FILES {
        return Err(DomainError::ResourceLimit(
            "too many package files".to_string(),
        ));
    }
    let mut seen = HashSet::new();
    let mut total_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| DomainError::InvalidResource(error.to_string()))?;
        if entry.is_symlink() {
            return Err(DomainError::InvalidResource(format!(
                "archive symlink entry is not allowed: {}",
                entry.name()
            )));
        }
        let raw_name = entry.name().to_string();
        let is_directory = entry.is_dir() || raw_name.ends_with('/') || raw_name.ends_with('\\');
        let (relative, path) =
            resolve_resource_path(destination, raw_name.trim_end_matches(['/', '\\']))?;
        if !seen.insert(relative.clone()) {
            return Err(DomainError::InvalidResource(format!(
                "archive contains duplicate path: {}",
                relative.display()
            )));
        }
        if is_directory {
            fs::create_dir_all(&path).map_err(|error| DomainError::Io(error.to_string()))?;
            continue;
        }
        reject_executable_extension(&relative)?;
        let size = entry.size();
        if size > MAX_FILE_BYTES {
            return Err(DomainError::ResourceLimit(format!(
                "archive entry is too large: {}",
                relative.display()
            )));
        }
        total_bytes = total_bytes
            .checked_add(size)
            .ok_or_else(|| DomainError::ResourceLimit("archive size overflow".to_string()))?;
        if total_bytes > MAX_TOTAL_BYTES {
            return Err(DomainError::ResourceLimit(
                "archive is too large".to_string(),
            ));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| DomainError::Io(error.to_string()))?;
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| DomainError::Io(error.to_string()))?;
        io::copy(&mut entry, &mut output).map_err(|error| DomainError::Io(error.to_string()))?;
    }
    Ok(())
}

fn collect_package_files(
    root: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), DomainError> {
    for entry in fs::read_dir(directory).map_err(|error| DomainError::Io(error.to_string()))? {
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
            collect_package_files(root, &path, files)?;
        } else if metadata.is_file() {
            files.push(relative);
        } else {
            return Err(DomainError::InvalidResource(
                "package contains a non-regular file".to_string(),
            ));
        }
    }
    Ok(())
}

fn reject_executable_extension(path: &Path) -> Result<(), DomainError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(
        extension.as_str(),
        "exe"
            | "com"
            | "bat"
            | "cmd"
            | "ps1"
            | "psm1"
            | "vbs"
            | "js"
            | "sh"
            | "bash"
            | "dll"
            | "so"
            | "dylib"
    ) {
        return Err(DomainError::InvalidResource(format!(
            "executable asset is not allowed: {}",
            path.display()
        )));
    }
    Ok(())
}
