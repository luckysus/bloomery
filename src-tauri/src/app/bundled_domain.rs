use super::domain_commands::DomainInstallResult;
use crate::db::{current_workspace_id, with_conn, with_conn_mut, DbState};
use crate::domains::{self, official_trust_store, DomainTrustStore};
use crate::storage::repositories::domains as domain_repository;
use std::path::{Path, PathBuf};
use tauri::Manager;

const BUNDLED_STEEL_PACKAGE_ID: &str = "steel";
const BUNDLED_STEEL_PACKAGE_RELATIVE_PATH: &str = "domain-packs/steel";

fn trust_store() -> DomainTrustStore {
    official_trust_store()
}

fn domains_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let directory = crate::db::app_data_directory(app)?.join("domains");
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("create domain package directory failed: {error}"))?;
    Ok(directory)
}

fn bundled_steel_package_candidates(resource_dir: &Path) -> [PathBuf; 3] {
    [
        resource_dir.join(BUNDLED_STEEL_PACKAGE_RELATIVE_PATH),
        resource_dir
            .join("resources")
            .join(BUNDLED_STEEL_PACKAGE_RELATIVE_PATH),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(BUNDLED_STEEL_PACKAGE_RELATIVE_PATH),
    ]
}

fn select_existing_directory(candidates: &[PathBuf]) -> Result<PathBuf, String> {
    candidates
        .iter()
        .find(|candidate| candidate.is_dir())
        .cloned()
        .ok_or_else(|| {
            let checked = candidates
                .iter()
                .map(|candidate| candidate.display().to_string())
                .collect::<Vec<_>>()
                .join("; ");
            format!("bundled steel domain package resource is missing; checked: {checked}")
        })
}

fn bundled_steel_package_source(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|error| format!("resolve resource directory failed: {error}"))?;
    select_existing_directory(&bundled_steel_package_candidates(&resource_dir))
}

fn active_version(
    connection: &rusqlite::Connection,
    workspace_id: &str,
    package_id: &str,
) -> Result<Option<String>, String> {
    Ok(domain_repository::list(connection, workspace_id)?
        .into_iter()
        .find(|record| record.id == package_id && record.active)
        .map(|record| record.version))
}

fn should_ensure_bundled_steel_package<I>(packages: I) -> bool
where
    I: IntoIterator<Item = (String, bool)>,
{
    !packages
        .into_iter()
        .any(|(package_id, active)| package_id == BUNDLED_STEEL_PACKAGE_ID && active)
}

fn rollback_installed_package(
    db: &tauri::State<'_, DbState>,
    package: &domains::InstalledDomainPackage,
) {
    let _ = with_conn_mut(db, |connection| {
        domain_repository::remove(
            connection,
            current_workspace_id(),
            &package.manifest.id,
            &package.manifest.version,
        )
    });
    let _ = std::fs::remove_dir_all(&package.path);
}

pub(crate) fn ensure_bundled_steel_package(
    app: &tauri::AppHandle,
    db: &tauri::State<'_, DbState>,
) -> Result<(), String> {
    let should_ensure = with_conn(db, |connection| {
        Ok(should_ensure_bundled_steel_package(
            domain_repository::list(connection, current_workspace_id())?
                .into_iter()
                .map(|package| (package.id, package.active)),
        ))
    })?;
    if should_ensure {
        install_steel_package(app, db).map(|_| ())?;
    }
    Ok(())
}

pub(crate) fn install_steel_package(
    app: &tauri::AppHandle,
    db: &tauri::State<DbState>,
) -> Result<DomainInstallResult, String> {
    let source = bundled_steel_package_source(app)?;
    let app_version = env!("CARGO_PKG_VERSION");
    let bundled = domains::load_package(&source, app_version).map_err(|error| error.to_string())?;
    if bundled.manifest.id != BUNDLED_STEEL_PACKAGE_ID {
        return Err(format!(
            "bundled domain package must have id {BUNDLED_STEEL_PACKAGE_ID}"
        ));
    }
    let source_digest =
        domains::compute_package_digest(&source).map_err(|error| error.to_string())?;
    let root = domains_root(app)?;
    let existing = with_conn(db, |connection| {
        domain_repository::get(
            connection,
            current_workspace_id(),
            &bundled.manifest.id,
            &bundled.manifest.version,
        )
    })?;

    if let Some(record) = existing {
        let installed_path = root.join(&record.id).join(&record.version);
        if PathBuf::from(&record.path) != installed_path {
            return Err("installed bundled domain package path is invalid".to_string());
        }
        let installed_digest = domains::compute_package_digest(&installed_path)
            .map_err(|error| format!("verify installed steel domain package failed: {error}"))?;
        if record.package_sha256 != source_digest || installed_digest != source_digest {
            return Err(
                "installed steel domain package does not match the bundled resource".to_string(),
            );
        }
        domains::load_package(&installed_path, app_version)
            .map_err(|error| format!("validate installed steel domain package failed: {error}"))?;
        if record.active {
            return Ok(DomainInstallResult {
                package: record,
                replaced_active_version: None,
            });
        }
        let replaced_active_version = with_conn(db, |connection| {
            active_version(connection, current_workspace_id(), &record.id)
        })?;
        domains::activate_package(&root, &record.id, &record.version, app_version)
            .map_err(|error| format!("activate bundled steel domain package failed: {error}"))?;
        let package = with_conn_mut(db, |connection| {
            domain_repository::activate(
                connection,
                current_workspace_id(),
                &record.id,
                &record.version,
            )
        })?;
        return Ok(DomainInstallResult {
            package,
            replaced_active_version,
        });
    }

    let replaced_active_version = with_conn(db, |connection| {
        active_version(connection, current_workspace_id(), &bundled.manifest.id)
    })?;
    let installed = domains::install_package(&source, &root, app_version, &trust_store())
        .map_err(|error| error.to_string())?;
    let package = match with_conn_mut(db, |connection| {
        domain_repository::upsert(connection, current_workspace_id(), &installed)
    }) {
        Ok(package) => package,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&installed.path);
            return Err(error);
        }
    };
    domains::activate_package(&root, &package.id, &package.version, app_version).map_err(
        |error| {
            rollback_installed_package(db, &installed);
            format!("activate bundled steel domain package failed: {error}")
        },
    )?;
    let package = match with_conn_mut(db, |connection| {
        domain_repository::activate(
            connection,
            current_workspace_id(),
            &package.id,
            &package.version,
        )
    }) {
        Ok(package) => package,
        Err(error) => {
            rollback_installed_package(db, &installed);
            return Err(error);
        }
    };
    Ok(DomainInstallResult {
        package,
        replaced_active_version,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        bundled_steel_package_candidates, select_existing_directory,
        should_ensure_bundled_steel_package, BUNDLED_STEEL_PACKAGE_ID,
    };
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    #[test]
    fn startup_does_not_replace_an_active_steel_domain_package() {
        assert!(!should_ensure_bundled_steel_package(vec![(
            BUNDLED_STEEL_PACKAGE_ID.to_string(),
            true,
        )]));
        assert!(should_ensure_bundled_steel_package(vec![(
            "other-domain".to_string(),
            true,
        )]));
        assert!(should_ensure_bundled_steel_package(vec![(
            BUNDLED_STEEL_PACKAGE_ID.to_string(),
            false,
        )]));
    }

    #[test]
    fn bundled_package_path_prefers_resource_and_reports_missing_resources() {
        let root = std::env::temp_dir().join(format!("bloomery-bundled-domain-{}", Uuid::new_v4()));
        let resource_dir = root.join("resource");
        let fallback = root.join("fallback");
        fs::create_dir_all(&fallback).expect("create fallback");

        let candidates = bundled_steel_package_candidates(&resource_dir);
        assert_eq!(
            candidates[0],
            resource_dir.join("domain-packs").join("steel")
        );
        assert!(select_existing_directory(&[PathBuf::from("missing"), fallback.clone()]).is_ok());

        let bundled = resource_dir.join("domain-packs").join("steel");
        fs::create_dir_all(&bundled).expect("create bundled resource");
        assert_eq!(
            select_existing_directory(&[bundled.clone(), fallback]).expect("select resource"),
            bundled
        );
        assert!(select_existing_directory(&[PathBuf::from("missing")]).is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn bundled_package_path_supports_nested_windows_resource_layout() {
        let root =
            std::env::temp_dir().join(format!("bloomery-bundled-domain-nested-{}", Uuid::new_v4()));
        let resource_dir = root.join("resource");
        let nested = resource_dir
            .join("resources")
            .join("domain-packs")
            .join("steel");
        fs::create_dir_all(&nested).expect("create nested bundled resource");

        let candidates = bundled_steel_package_candidates(&resource_dir);
        assert!(
            candidates.iter().any(|candidate| candidate == &nested),
            "nested Tauri resource layout must be searched"
        );
        assert_eq!(
            select_existing_directory(&candidates).expect("select nested resource"),
            nested
        );

        let _ = fs::remove_dir_all(root);
    }
}
