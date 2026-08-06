use super::manifest::{AssetSpec, DomainManifest};
use serde::Deserialize;
use std::collections::HashSet;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Component, Path, PathBuf};

const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const MAX_ASSET_COUNT: usize = 512;
const MAX_ASSET_BYTES: u64 = 32 * 1024 * 1024;
const MAX_TOTAL_ASSET_BYTES: u64 = 512 * 1024 * 1024;
const DEFAULT_APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    Io(String),
    InvalidManifest(String),
    Incompatible(String),
    UnsafePath(String),
    InvalidResource(String),
    ResourceLimit(String),
    Signature(String),
}

impl Display for DomainError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(formatter, "domain_io: {message}"),
            Self::InvalidManifest(message) => {
                write!(formatter, "domain_manifest_invalid: {message}")
            }
            Self::Incompatible(message) => write!(formatter, "domain_incompatible: {message}"),
            Self::UnsafePath(message) => write!(formatter, "domain_unsafe_path: {message}"),
            Self::InvalidResource(message) => {
                write!(formatter, "domain_resource_invalid: {message}")
            }
            Self::ResourceLimit(message) => write!(formatter, "domain_resource_limit: {message}"),
            Self::Signature(message) => write!(formatter, "domain_signature_invalid: {message}"),
        }
    }
}

impl std::error::Error for DomainError {}

#[derive(Debug, Clone)]
pub struct ResolvedAsset {
    pub spec: AssetSpec,
    pub relative_path: PathBuf,
    pub path: PathBuf,
    pub size: u64,
}

#[derive(Debug, Clone)]
pub struct LoadedDomainPackage {
    pub root: PathBuf,
    pub manifest: DomainManifest,
    pub assets: Vec<ResolvedAsset>,
}

pub fn load_package(root: &Path, app_version: &str) -> Result<LoadedDomainPackage, DomainError> {
    if !root.is_dir() {
        return Err(DomainError::Io(
            "package root is not a directory".to_string(),
        ));
    }
    let manifest_path = root.join("manifest.json");
    let bytes = fs::read(&manifest_path).map_err(|error| DomainError::Io(error.to_string()))?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(DomainError::ResourceLimit(
            "manifest is too large".to_string(),
        ));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| DomainError::InvalidManifest("manifest is not UTF-8".to_string()))?;
    let manifest = serde_json::from_str::<DomainManifest>(&text)
        .map_err(|error| DomainError::InvalidManifest(error.to_string()))?;
    validate_manifest(&manifest, app_version)?;

    if manifest.assets.len() > MAX_ASSET_COUNT {
        return Err(DomainError::ResourceLimit(
            "too many package assets".to_string(),
        ));
    }
    let mut assets = Vec::with_capacity(manifest.assets.len());
    let mut total_bytes = 0_u64;
    let mut seen = HashSet::new();
    for spec in &manifest.assets {
        let (relative_path, path) = resolve_resource_path(root, &spec.path)?;
        if !seen.insert(relative_path.clone()) {
            return Err(DomainError::InvalidResource(format!(
                "asset is declared more than once: {}",
                spec.path
            )));
        }
        reject_executable_path(&relative_path)?;
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| DomainError::InvalidResource(format!("{}: {error}", spec.path)))?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(DomainError::InvalidResource(format!(
                "asset is not a regular file: {}",
                spec.path
            )));
        }
        let root_canonical =
            fs::canonicalize(root).map_err(|error| DomainError::Io(error.to_string()))?;
        let asset_canonical = fs::canonicalize(&path)
            .map_err(|error| DomainError::InvalidResource(format!("{}: {error}", spec.path)))?;
        if !asset_canonical.starts_with(&root_canonical) {
            return Err(DomainError::UnsafePath(spec.path.clone()));
        }
        let size = metadata.len();
        if size > MAX_ASSET_BYTES {
            return Err(DomainError::ResourceLimit(format!(
                "asset is too large: {}",
                spec.path
            )));
        }
        total_bytes = total_bytes
            .checked_add(size)
            .ok_or_else(|| DomainError::ResourceLimit("asset size overflow".to_string()))?;
        if total_bytes > MAX_TOTAL_ASSET_BYTES {
            return Err(DomainError::ResourceLimit(
                "package assets are too large".to_string(),
            ));
        }
        assets.push(ResolvedAsset {
            spec: spec.clone(),
            relative_path,
            path,
            size,
        });
    }
    Ok(LoadedDomainPackage {
        root: root.to_path_buf(),
        manifest,
        assets,
    })
}

pub fn resolve_resource_path(
    root: &Path,
    raw_path: &str,
) -> Result<(PathBuf, PathBuf), DomainError> {
    let value = raw_path.trim();
    if value.is_empty() || value != raw_path || value.contains('\0') {
        return Err(DomainError::UnsafePath(raw_path.to_string()));
    }
    if value.starts_with('/') || value.starts_with('\\') || is_windows_absolute(value) {
        return Err(DomainError::UnsafePath(raw_path.to_string()));
    }
    let relative = Path::new(value);
    let mut normalized = PathBuf::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            _ => return Err(DomainError::UnsafePath(raw_path.to_string())),
        }
    }
    if normalized.as_os_str().is_empty() {
        return Err(DomainError::UnsafePath(raw_path.to_string()));
    }
    let path = root.join(&normalized);
    if !path.starts_with(root) {
        return Err(DomainError::UnsafePath(raw_path.to_string()));
    }
    Ok((normalized, path))
}

fn validate_manifest(manifest: &DomainManifest, app_version: &str) -> Result<(), DomainError> {
    validate_identifier(&manifest.id, "id")?;
    validate_version(&manifest.version, "version")?;
    validate_version(
        &manifest.compatibility.min_app_version,
        "compatibility.min_app_version",
    )?;
    if let Some(maximum) = &manifest.compatibility.max_app_version {
        validate_version(maximum, "compatibility.max_app_version")?;
    }
    let current = parse_version(app_version)
        .ok_or_else(|| DomainError::InvalidManifest("app version is invalid".to_string()))?;
    let minimum = parse_version(&manifest.compatibility.min_app_version).unwrap();
    let maximum = manifest
        .compatibility
        .max_app_version
        .as_deref()
        .map(|value| parse_version(value).unwrap());
    if current < minimum || maximum.is_some_and(|value| current > value) {
        return Err(DomainError::Incompatible(format!(
            "package requires Bloomery {}..{:?}",
            manifest.compatibility.min_app_version, manifest.compatibility.max_app_version
        )));
    }
    if manifest.author.trim().is_empty() || !allowed_license(&manifest.license) {
        return Err(DomainError::InvalidManifest(
            "author or license is invalid".to_string(),
        ));
    }
    if manifest.prompts.system.trim().is_empty() || manifest.prompts.workflow.trim().is_empty() {
        return Err(DomainError::InvalidManifest(
            "prompts must not be empty".to_string(),
        ));
    }
    if !(1..=100).contains(&manifest.retrieval.max_evidence_items) {
        return Err(DomainError::InvalidManifest(
            "retrieval.max_evidence_items is out of range".to_string(),
        ));
    }
    for tag in &manifest.retrieval.required_tags {
        validate_identifier(tag, "retrieval.required_tags")?;
    }
    for tool in &manifest.builtin_tool_allowlist {
        validate_identifier(tool, "builtin_tool_allowlist")?;
    }
    for recommendation in &manifest.mcp_recommendations {
        validate_identifier(&recommendation.id, "mcp_recommendations.id")?;
        if !matches!(
            recommendation.transport.as_str(),
            "stdio" | "sse" | "streamable_http"
        ) {
            return Err(DomainError::InvalidManifest(
                "MCP transport is unsupported".to_string(),
            ));
        }
        if recommendation.description.trim().is_empty() {
            return Err(DomainError::InvalidManifest(
                "MCP recommendation description is empty".to_string(),
            ));
        }
    }
    for mapping in &manifest.data_mappings {
        validate_identifier(&mapping.dataset, "data_mappings.dataset")?;
        for (field, target) in mapping.fields.iter().chain(mapping.units.iter()) {
            if field.trim().is_empty() || target.trim().is_empty() {
                return Err(DomainError::InvalidManifest(
                    "data mapping contains an empty field".to_string(),
                ));
            }
        }
    }
    for evaluation in &manifest.evaluations {
        validate_identifier(&evaluation.id, "evaluations.id")?;
        validate_identifier(&evaluation.kind, "evaluations.kind")?;
        if evaluation.dataset.trim().is_empty() || evaluation.expected_behavior.trim().is_empty() {
            return Err(DomainError::InvalidManifest(
                "evaluation fields must not be empty".to_string(),
            ));
        }
        if evaluation
            .threshold
            .is_some_and(|value| !(0.0..=1.0).contains(&value))
        {
            return Err(DomainError::InvalidManifest(
                "evaluation threshold is out of range".to_string(),
            ));
        }
    }
    Ok(())
}

fn reject_executable_path(path: &Path) -> Result<(), DomainError> {
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

fn validate_identifier(value: &str, field: &str) -> Result<(), DomainError> {
    if value.is_empty()
        || value.len() > 96
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_' | b'.')
        })
    {
        return Err(DomainError::InvalidManifest(format!("{field} is invalid")));
    }
    Ok(())
}

fn validate_version(value: &str, field: &str) -> Result<(), DomainError> {
    if parse_version(value).is_none() {
        return Err(DomainError::InvalidManifest(format!(
            "{field} must be major.minor.patch"
        )));
    }
    Ok(())
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.split('.');
    let parsed = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(parsed)
}

fn allowed_license(value: &str) -> bool {
    matches!(
        value,
        "MIT" | "Apache-2.0" | "BSD-3-Clause" | "MPL-2.0" | "CC-BY-4.0" | "CC0-1.0"
    )
}

fn is_windows_absolute(value: &str) -> bool {
    value.as_bytes().get(1) == Some(&b':')
}

#[allow(dead_code)]
fn _default_app_version() -> &'static str {
    DEFAULT_APP_VERSION
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct _ManifestShapeGuard {
    _unused: Option<String>,
}
