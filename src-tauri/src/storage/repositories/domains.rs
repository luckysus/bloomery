use crate::domains::{DomainManifest, DomainTrust, InstalledDomainPackage};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainPackageRecord {
    pub id: String,
    pub version: String,
    pub path: String,
    pub package_sha256: String,
    pub trust: DomainTrust,
    pub manifest: DomainManifest,
    pub installed_at: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DomainPackageImpact {
    pub package_id: String,
    pub version: String,
    pub active: bool,
    pub tool_count: usize,
    pub mcp_recommendation_count: usize,
    pub asset_count: usize,
}

pub fn upsert(
    connection: &mut Connection,
    workspace_id: &str,
    package: &InstalledDomainPackage,
) -> Result<DomainPackageRecord, String> {
    let manifest_json =
        serde_json::to_string(&package.manifest).map_err(|error| error.to_string())?;
    let installed_at = Utc::now().to_rfc3339();
    connection
        .execute(
            "INSERT INTO domain_packages
               (workspace_id, id, version, path, package_sha256, trust, manifest_json,
                installed_at, active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0)",
            params![
                workspace_id,
                package.manifest.id,
                package.manifest.version,
                package.path.to_string_lossy().to_string(),
                package.package_sha256,
                trust_value(package.trust),
                manifest_json,
                installed_at,
            ],
        )
        .map_err(|error| error.to_string())?;
    get(
        connection,
        workspace_id,
        &package.manifest.id,
        &package.manifest.version,
    )?
    .ok_or_else(|| "domain package record was not created".to_string())
}

pub fn list(
    connection: &Connection,
    workspace_id: &str,
) -> Result<Vec<DomainPackageRecord>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, version, path, package_sha256, trust, manifest_json, installed_at, active
             FROM domain_packages
             WHERE workspace_id = ?1
             ORDER BY id, version",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![workspace_id], map_record)
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

/// Return the manifest of the currently active domain package for the workspace, if any.
///
/// Reuses [`list`] and selects the single record whose `active` flag is set; activation
/// guarantees at most one active version per package id.
pub fn active_manifest(
    connection: &Connection,
    workspace_id: &str,
) -> Result<Option<DomainManifest>, String> {
    Ok(list(connection, workspace_id)?
        .into_iter()
        .find(|record| record.active)
        .map(|record| record.manifest))
}

pub fn get(
    connection: &Connection,
    workspace_id: &str,
    package_id: &str,
    version: &str,
) -> Result<Option<DomainPackageRecord>, String> {
    connection
        .query_row(
            "SELECT id, version, path, package_sha256, trust, manifest_json, installed_at, active
             FROM domain_packages
             WHERE workspace_id = ?1 AND id = ?2 AND version = ?3",
            params![workspace_id, package_id, version],
            map_record,
        )
        .optional()
        .map_err(|error| error.to_string())
}

pub fn activate(
    connection: &mut Connection,
    workspace_id: &str,
    package_id: &str,
    version: &str,
) -> Result<DomainPackageRecord, String> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let exists = transaction
        .query_row(
            "SELECT 1 FROM domain_packages
             WHERE workspace_id = ?1 AND id = ?2 AND version = ?3",
            params![workspace_id, package_id, version],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .is_some();
    if !exists {
        return Err("domain package version is not installed".to_string());
    }
    transaction
        .execute(
            "UPDATE domain_packages SET active = 0
             WHERE workspace_id = ?1 AND id = ?2",
            params![workspace_id, package_id],
        )
        .map_err(|error| error.to_string())?;
    transaction
        .execute(
            "UPDATE domain_packages SET active = 1
             WHERE workspace_id = ?1 AND id = ?2 AND version = ?3",
            params![workspace_id, package_id, version],
        )
        .map_err(|error| error.to_string())?;
    transaction.commit().map_err(|error| error.to_string())?;
    get(connection, workspace_id, package_id, version)?
        .ok_or_else(|| "activated domain package disappeared".to_string())
}

pub fn impact(
    connection: &Connection,
    workspace_id: &str,
    package_id: &str,
    version: &str,
) -> Result<DomainPackageImpact, String> {
    let record = get(connection, workspace_id, package_id, version)?
        .ok_or_else(|| "domain package version is not installed".to_string())?;
    Ok(DomainPackageImpact {
        package_id: record.id,
        version: record.version,
        active: record.active,
        tool_count: record.manifest.builtin_tool_allowlist.len(),
        mcp_recommendation_count: record.manifest.mcp_recommendations.len(),
        asset_count: record.manifest.assets.len(),
    })
}

pub fn remove(
    connection: &mut Connection,
    workspace_id: &str,
    package_id: &str,
    version: &str,
) -> Result<(), String> {
    let active = get(connection, workspace_id, package_id, version)?
        .ok_or_else(|| "domain package version is not installed".to_string())?
        .active;
    if active {
        return Err("active domain package cannot be removed".to_string());
    }
    let deleted = connection
        .execute(
            "DELETE FROM domain_packages
             WHERE workspace_id = ?1 AND id = ?2 AND version = ?3",
            params![workspace_id, package_id, version],
        )
        .map_err(|error| error.to_string())?;
    if deleted != 1 {
        return Err("domain package version is not installed".to_string());
    }
    Ok(())
}

pub fn restore(
    connection: &mut Connection,
    workspace_id: &str,
    package: &DomainPackageRecord,
) -> Result<(), String> {
    let manifest_json =
        serde_json::to_string(&package.manifest).map_err(|error| error.to_string())?;
    let inserted = connection
        .execute(
            "INSERT INTO domain_packages
               (workspace_id, id, version, path, package_sha256, trust, manifest_json,
                installed_at, active)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                workspace_id,
                package.id,
                package.version,
                package.path,
                package.package_sha256,
                trust_value(package.trust),
                manifest_json,
                package.installed_at,
                if package.active { 1_i64 } else { 0_i64 },
            ],
        )
        .map_err(|error| error.to_string())?;
    if inserted != 1 {
        return Err("domain package record was not restored".to_string());
    }
    Ok(())
}

fn map_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<DomainPackageRecord> {
    let trust: String = row.get(4)?;
    let manifest_json: String = row.get(5)?;
    Ok(DomainPackageRecord {
        id: row.get(0)?,
        version: row.get(1)?,
        path: row.get(2)?,
        package_sha256: row.get(3)?,
        trust: parse_trust(&trust).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
            )
        })?,
        manifest: serde_json::from_str::<DomainManifest>(&manifest_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        installed_at: row.get(6)?,
        active: row.get::<_, i64>(7)? != 0,
    })
}

fn trust_value(trust: DomainTrust) -> &'static str {
    match trust {
        DomainTrust::OfficialSigned => "official_signed",
        DomainTrust::ThirdPartyUnsigned => "third_party_unsigned",
    }
}

fn parse_trust(value: &str) -> Result<DomainTrust, String> {
    match value {
        "official_signed" => Ok(DomainTrust::OfficialSigned),
        "third_party_unsigned" => Ok(DomainTrust::ThirdPartyUnsigned),
        _ => Err(format!("unknown domain package trust: {value}")),
    }
}
