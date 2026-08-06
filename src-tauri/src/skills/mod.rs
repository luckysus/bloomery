use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const MAX_SKILL_BYTES: usize = 1024 * 1024;
const MAX_ENABLED_SKILLS: usize = 12;
const MAX_RENDERED_SKILL_PROMPT_CHARS: usize = 12_000;
const MAX_RENDERED_SKILL_BODY_CHARS: usize = 8_000;
pub const SKILLS_SETTING_KEY: &str = "extensions.skills.enabled";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    User,
    Workspace,
    Domain,
}

impl SkillScope {
    fn priority(self) -> u8 {
        match self {
            Self::User => 0,
            Self::Workspace => 1,
            Self::Domain => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRoot {
    pub scope: SkillScope,
    pub path: PathBuf,
}

impl SkillRoot {
    pub fn new(scope: SkillScope, path: PathBuf) -> Self {
        Self { scope, path }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSource {
    pub scope: SkillScope,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillRecord {
    pub name: String,
    pub description: String,
    pub version: String,
    pub compatibility: Vec<String>,
    pub body: String,
    pub source: SkillSource,
    pub content_sha256: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillErrorCode {
    ReadFailed,
    Oversized,
    InvalidUtf8,
    MissingFrontmatter,
    InvalidFrontmatter,
    MissingField,
    InvalidField,
    InvalidName,
    InvalidVersion,
    InvalidCompatibility,
    Incompatible,
    Duplicate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillLoadError {
    pub code: SkillErrorCode,
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillDiscoveryReport {
    pub skills: Vec<SkillRecord>,
    pub errors: Vec<SkillLoadError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub version: String,
    pub compatibility: Vec<String>,
    pub source: SkillSource,
    pub content_sha256: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillCatalog {
    pub skills: Vec<SkillSummary>,
    pub errors: Vec<SkillLoadError>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderedSkills {
    pub prompt: String,
    pub enabled_versions: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SkillContext {
    pub summaries: Vec<SkillSummary>,
    pub rendered: RenderedSkills,
    pub errors: Vec<SkillLoadError>,
}

pub fn discover_skills(roots: &[SkillRoot], app_version: &str) -> SkillDiscoveryReport {
    let mut roots = roots.to_vec();
    roots.sort_by(|left, right| {
        left.scope
            .priority()
            .cmp(&right.scope.priority())
            .then_with(|| {
                left.path
                    .to_string_lossy()
                    .cmp(&right.path.to_string_lossy())
            })
    });

    let mut report = SkillDiscoveryReport::default();
    let mut selected = BTreeMap::<String, SkillRecord>::new();
    for root in roots {
        let mut directories = match fs::read_dir(&root.path) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
                .collect::<Vec<_>>(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                report.errors.push(SkillLoadError {
                    code: SkillErrorCode::ReadFailed,
                    path: root.path,
                    message: error.to_string(),
                });
                continue;
            }
        };
        directories.sort_by_key(|entry| entry.file_name());

        for directory in directories {
            let path = directory.path().join("SKILL.md");
            if !path.is_file() {
                continue;
            }
            let Some(skill) = load_skill(&path, root.scope, app_version, &mut report.errors) else {
                continue;
            };
            if selected.contains_key(&skill.name) {
                report.errors.push(SkillLoadError {
                    code: SkillErrorCode::Duplicate,
                    path: skill.source.path.clone(),
                    message: format!(
                        "skill '{}' is shadowed by a higher-precedence source",
                        skill.name
                    ),
                });
                continue;
            }
            selected.insert(skill.name.clone(), skill);
        }
    }
    report.skills = selected.into_values().collect();
    report
}

pub fn default_skill_roots() -> Vec<SkillRoot> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        push_root(
            &mut roots,
            SkillScope::User,
            home.join(".claude").join("skills"),
        );
    }
    let workspace = std::env::var_os("BLOOMERY_WORKSPACE_PATH")
        .map(PathBuf::from)
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| std::env::current_dir().ok());
    if let Some(workspace) = workspace {
        push_root(
            &mut roots,
            SkillScope::Workspace,
            workspace.join(".claude").join("skills"),
        );
    }
    if let Some(data_dir) = dirs::data_local_dir() {
        push_root(
            &mut roots,
            SkillScope::Domain,
            data_dir
                .join("Bloomery")
                .join("domains")
                .join("active")
                .join("skills"),
        );
    }
    roots
}

fn push_root(roots: &mut Vec<SkillRoot>, scope: SkillScope, path: PathBuf) {
    if !roots
        .iter()
        .any(|root| root.scope == scope && root.path == path)
    {
        roots.push(SkillRoot::new(scope, path));
    }
}

pub fn summarize_skills(
    skills: &[SkillRecord],
    enabled_names: &BTreeSet<String>,
) -> Vec<SkillSummary> {
    skills
        .iter()
        .map(|skill| SkillSummary {
            name: skill.name.clone(),
            description: skill.description.clone(),
            version: skill.version.clone(),
            compatibility: skill.compatibility.clone(),
            source: skill.source.clone(),
            content_sha256: skill.content_sha256.clone(),
            enabled: enabled_names.contains(&skill.name),
        })
        .collect()
}

pub fn render_enabled_skills(
    skills: &[SkillRecord],
    enabled_names: &BTreeSet<String>,
) -> RenderedSkills {
    let mut prompt_sections = Vec::new();
    let mut enabled_versions = Vec::new();
    let mut prompt_chars = "enabled_skills:".chars().count();

    for skill in skills
        .iter()
        .filter(|skill| enabled_names.contains(&skill.name))
        .take(MAX_ENABLED_SKILLS)
    {
        let version = format!("{}@{}#{}", skill.name, skill.version, skill.content_sha256);
        let header = format!("## {} (v{})", skill.name, skill.version);
        let available = MAX_RENDERED_SKILL_PROMPT_CHARS
            .saturating_sub(prompt_chars + header.chars().count() + 4);
        if available == 0 {
            break;
        }
        let body = truncate_text(&skill.body, available.min(MAX_RENDERED_SKILL_BODY_CHARS));
        let section = format!("{header}\n{body}");
        prompt_chars += section.chars().count() + 2;
        prompt_sections.push(section);
        enabled_versions.push(version);
    }

    let prompt = if prompt_sections.is_empty() {
        String::new()
    } else {
        format!("enabled_skills:\n\n{}", prompt_sections.join("\n\n"))
    };
    RenderedSkills {
        prompt,
        enabled_versions,
    }
}

pub fn load_enabled_names(
    connection: &rusqlite::Connection,
    workspace_id: &str,
) -> Result<BTreeSet<String>, String> {
    let raw =
        crate::storage::repositories::settings::get(connection, workspace_id, SKILLS_SETTING_KEY)?;
    Ok(parse_enabled_names(raw.as_deref()))
}

pub fn parse_enabled_names(raw: Option<&str>) -> BTreeSet<String> {
    let Some(raw) = raw else {
        return BTreeSet::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        return BTreeSet::new();
    };
    value
        .get("enabled")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .filter(|name| valid_name(name))
        .map(str::to_string)
        .collect()
}

pub fn save_enabled_names(
    connection: &mut rusqlite::Connection,
    workspace_id: &str,
    names: &BTreeSet<String>,
) -> Result<(), String> {
    let value = serde_json::json!({
        "version": 1,
        "enabled": names.iter().collect::<Vec<_>>(),
    });
    crate::storage::repositories::settings::set(
        connection,
        workspace_id,
        SKILLS_SETTING_KEY,
        &value.to_string(),
    )
}

pub fn load_context(
    connection: &rusqlite::Connection,
    workspace_id: &str,
    app_version: &str,
) -> Result<SkillContext, String> {
    let enabled_names = load_enabled_names(connection, workspace_id)?;
    let report = discover_skills(&default_skill_roots(), app_version);
    Ok(SkillContext {
        summaries: summarize_skills(&report.skills, &enabled_names),
        rendered: render_enabled_skills(&report.skills, &enabled_names),
        errors: report.errors,
    })
}

pub fn catalog(
    connection: &rusqlite::Connection,
    workspace_id: &str,
    app_version: &str,
) -> Result<SkillCatalog, String> {
    let context = load_context(connection, workspace_id, app_version)?;
    Ok(SkillCatalog {
        skills: context.summaries,
        errors: context.errors,
    })
}

pub fn set_enabled(
    connection: &mut rusqlite::Connection,
    workspace_id: &str,
    name: &str,
    enabled: bool,
    app_version: &str,
) -> Result<SkillCatalog, String> {
    let name = name.trim();
    if !valid_name(name) {
        return Err("skill name is invalid".to_string());
    }
    let report = discover_skills(&default_skill_roots(), app_version);
    if !report.skills.iter().any(|skill| skill.name == name) {
        return Err("skill not found".to_string());
    }
    let mut names = load_enabled_names(connection, workspace_id)?;
    if enabled {
        names.insert(name.to_string());
    } else {
        names.remove(name);
    }
    save_enabled_names(connection, workspace_id, &names)?;
    catalog(connection, workspace_id, app_version)
}

fn load_skill(
    path: &Path,
    scope: SkillScope,
    app_version: &str,
    errors: &mut Vec<SkillLoadError>,
) -> Option<SkillRecord> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            push_error(errors, SkillErrorCode::ReadFailed, path, error.to_string());
            return None;
        }
    };
    if bytes.len() > MAX_SKILL_BYTES {
        push_error(
            errors,
            SkillErrorCode::Oversized,
            path,
            "SKILL.md exceeds the size limit",
        );
        return None;
    }
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(_) => {
            push_error(
                errors,
                SkillErrorCode::InvalidUtf8,
                path,
                "SKILL.md is not valid UTF-8",
            );
            return None;
        }
    };
    let (fields, body) = match parse_frontmatter(&text) {
        Ok(value) => value,
        Err((code, message)) => {
            push_error(errors, code, path, message);
            return None;
        }
    };
    let Some(name) = required(&fields, "name", path, errors) else {
        return None;
    };
    if !valid_name(&name) {
        push_error(
            errors,
            SkillErrorCode::InvalidName,
            path,
            "skill name contains invalid characters",
        );
        return None;
    }
    let Some(description) = required(&fields, "description", path, errors) else {
        return None;
    };
    let version = fields
        .get("version")
        .cloned()
        .unwrap_or_else(|| "0.0.0".to_string());
    if !valid_version(&version) {
        push_error(
            errors,
            SkillErrorCode::InvalidVersion,
            path,
            "skill version must be major.minor.patch",
        );
        return None;
    }
    let compatibility = match fields.get("compatibility") {
        Some(value) => match parse_compatibility(value) {
            Ok(value) => value,
            Err(message) => {
                push_error(errors, SkillErrorCode::InvalidCompatibility, path, message);
                return None;
            }
        },
        None => Vec::new(),
    };
    match compatible(&compatibility, app_version) {
        Ok(true) => {}
        Ok(false) => {
            push_error(
                errors,
                SkillErrorCode::Incompatible,
                path,
                "skill is incompatible with this Bloomery version",
            );
            return None;
        }
        Err(message) => {
            push_error(errors, SkillErrorCode::InvalidCompatibility, path, message);
            return None;
        }
    }

    let digest = Sha256::digest(text.as_bytes());
    Some(SkillRecord {
        name,
        description,
        version,
        compatibility,
        body,
        source: SkillSource {
            scope,
            path: path.to_path_buf(),
        },
        content_sha256: digest.iter().map(|byte| format!("{byte:02x}")).collect(),
    })
}

fn parse_frontmatter(
    text: &str,
) -> Result<(BTreeMap<String, String>, String), (SkillErrorCode, String)> {
    let mut lines = text.lines();
    if lines.next().map(str::trim) != Some("---") {
        return Err((
            SkillErrorCode::MissingFrontmatter,
            "SKILL.md must start with frontmatter".to_string(),
        ));
    }
    let mut fields = BTreeMap::new();
    let mut closed = false;
    for line in lines.by_ref() {
        let line = line.trim();
        if line == "---" {
            closed = true;
            break;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err((
                SkillErrorCode::InvalidFrontmatter,
                "frontmatter lines must use key: value".to_string(),
            ));
        };
        let key = key.trim();
        let value = unquote(value.trim());
        if key.is_empty() || value.is_empty() {
            return Err((
                SkillErrorCode::InvalidFrontmatter,
                "frontmatter keys and values must not be empty".to_string(),
            ));
        }
        if fields.insert(key.to_string(), value).is_some() {
            return Err((
                SkillErrorCode::InvalidField,
                format!("frontmatter field '{key}' is duplicated"),
            ));
        }
    }
    if !closed {
        return Err((
            SkillErrorCode::InvalidFrontmatter,
            "frontmatter closing marker is missing".to_string(),
        ));
    }
    Ok((
        fields,
        lines.collect::<Vec<_>>().join("\n").trim().to_string(),
    ))
}

fn required(
    fields: &BTreeMap<String, String>,
    name: &str,
    path: &Path,
    errors: &mut Vec<SkillLoadError>,
) -> Option<String> {
    match fields.get(name) {
        Some(value) => Some(value.clone()),
        None => {
            push_error(
                errors,
                SkillErrorCode::MissingField,
                path,
                format!("frontmatter field '{name}' is missing"),
            );
            None
        }
    }
}

fn parse_compatibility(value: &str) -> Result<Vec<String>, String> {
    let value = value.trim();
    if value.is_empty() || value == "*" {
        return Ok(Vec::new());
    }
    let values = if value.starts_with('[') && value.ends_with(']') {
        value[1..value.len() - 1]
            .split(',')
            .map(|item| unquote(item.trim()))
            .collect::<Vec<_>>()
    } else {
        vec![unquote(value)]
    };
    if values.iter().any(|item| item.is_empty()) {
        return Err("compatibility contains an empty constraint".to_string());
    }
    for item in &values {
        let version = item
            .strip_prefix("bloomery")
            .unwrap_or(item)
            .trim_start_matches(">=")
            .trim_start_matches('=')
            .trim();
        if !valid_version(version) {
            return Err(format!("invalid compatibility constraint '{item}'"));
        }
    }
    Ok(values)
}

fn compatible(constraints: &[String], app_version: &str) -> Result<bool, String> {
    let current = parse_version(app_version)
        .ok_or_else(|| "Bloomery version must be major.minor.patch".to_string())?;
    for constraint in constraints {
        let version = constraint
            .strip_prefix("bloomery")
            .unwrap_or(constraint)
            .trim_start_matches(">=")
            .trim_start_matches('=')
            .trim();
        let required =
            parse_version(version).ok_or_else(|| "invalid compatibility version".to_string())?;
        if current < required {
            return Ok(false);
        }
    }
    Ok(true)
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn valid_version(value: &str) -> bool {
    parse_version(value).is_some()
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].trim().to_string()
    } else {
        value.to_string()
    }
}

fn push_error(
    errors: &mut Vec<SkillLoadError>,
    code: SkillErrorCode,
    path: &Path,
    message: impl Into<String>,
) {
    errors.push(SkillLoadError {
        code,
        path: path.to_path_buf(),
        message: message.into(),
    });
}

fn truncate_text(value: &str, limit: usize) -> String {
    let mut result = value.chars().take(limit).collect::<String>();
    if value.chars().count() > limit {
        result.push('\u{2026}');
    }
    result
}
