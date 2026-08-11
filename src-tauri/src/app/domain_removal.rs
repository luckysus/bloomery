use crate::permissions::path::AuthorizedRoots;
use crate::storage::repositories::domains as domain_repository;
use rusqlite::Connection;
use std::path::Path;
use uuid::Uuid;

pub fn remove_package_atomically(
    connection: &mut Connection,
    workspace_id: &str,
    domains_root: &Path,
    package_id: &str,
    version: &str,
) -> Result<(), String> {
    let record = domain_repository::get(connection, workspace_id, package_id, version)?
        .ok_or_else(|| "domain package version is not installed".to_string())?;
    if record.active {
        return Err("active domain package cannot be removed".to_string());
    }

    let root = std::fs::canonicalize(domains_root)
        .map_err(|error| format!("resolve domain package root failed: {error}"))?;
    let recorded_path = Path::new(&record.path);
    let metadata = std::fs::symlink_metadata(recorded_path)
        .map_err(|error| format!("resolve domain package files failed: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("domain package path must be a regular directory".to_string());
    }
    let authorized = AuthorizedRoots::new(vec![domains_root.to_path_buf()])
        .map_err(|error| error.to_string())?;
    let package_path = authorized
        .authorize(recorded_path)
        .map_err(|error| error.to_string())?
        .canonical_path()
        .to_path_buf();
    if package_path == root {
        return Err("domain package path cannot be the domains root".to_string());
    }

    let staging_root = root.join(".staging").join("remove");
    std::fs::create_dir_all(&staging_root)
        .map_err(|error| format!("create domain removal staging failed: {error}"))?;
    let staging = staging_root.join(Uuid::new_v4().to_string());
    std::fs::rename(&package_path, &staging)
        .map_err(|error| format!("stage domain package removal failed: {error}"))?;

    match domain_repository::remove(connection, workspace_id, package_id, version) {
        Ok(()) => {
            if let Err(error) = std::fs::remove_dir_all(&staging) {
                let restore_record = domain_repository::restore(connection, workspace_id, &record);
                let restore_files = std::fs::rename(&staging, &package_path);
                remove_empty_staging_directories(&staging_root);
                return Err(format!(
                    "remove domain package files failed: {error}; database restore: {}; file restore: {}",
                    format_restore_result(restore_record),
                    format_restore_result(restore_files),
                ));
            }
            remove_empty_staging_directories(&staging_root);
            Ok(())
        }
        Err(error) => {
            let restore = std::fs::rename(&staging, &package_path);
            remove_empty_staging_directories(&staging_root);
            if let Err(restore_error) = restore {
                return Err(format!(
                    "{error}; failed to restore staged domain package: {restore_error}"
                ));
            }
            Err(error)
        }
    }
}

fn remove_empty_staging_directories(staging_root: &Path) {
    let _ = std::fs::remove_dir(staging_root);
    if let Some(parent) = staging_root.parent() {
        let _ = std::fs::remove_dir(parent);
    }
}

fn format_restore_result<T, E: std::fmt::Display>(result: Result<T, E>) -> String {
    match result {
        Ok(_) => "ok".to_string(),
        Err(error) => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::remove_package_atomically;
    use crate::domains::{install_package, DomainTrustStore};
    use crate::storage::migrations::migrate;
    use crate::storage::repositories::domains::{get, upsert};
    use rusqlite::Connection;
    use serde_json::json;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn failed_database_delete_restores_the_staged_domain_package() {
        let source =
            std::env::temp_dir().join(format!("bloomery-domain-source-{}", Uuid::new_v4()));
        let root = std::env::temp_dir().join(format!("bloomery-domain-root-{}", Uuid::new_v4()));
        fs::create_dir_all(source.join("assets")).expect("create source");
        fs::write(source.join("assets/steel.json"), "{}").expect("write asset");
        fs::write(
            source.join("manifest.json"),
            serde_json::to_vec(&json!({
                "id": "steel",
                "version": "1.0.0",
                "compatibility": {"min_app_version": "0.1.0", "max_app_version": null},
                "author": "Bloomery",
                "license": "Apache-2.0",
                "prompts": {"system": "steel", "workflow": "cite"},
                "retrieval": {"required_tags": [], "citation_required": true, "max_evidence_items": 12},
                "assets": [{"path": "assets/steel.json", "kind": "terminology", "sha256": null}]
            }))
            .expect("serialize manifest"),
        )
        .expect("write manifest");
        fs::create_dir_all(&root).expect("create install root");
        let installed = install_package(&source, &root, "0.1.0", &DomainTrustStore::default())
            .expect("install package");
        let mut connection = Connection::open_in_memory().expect("open database");
        migrate(&mut connection).expect("migrate database");
        let record = upsert(&mut connection, "local", &installed).expect("persist package");
        connection
            .execute_batch(
                "CREATE TRIGGER reject_domain_delete
                 BEFORE DELETE ON domain_packages
                 BEGIN SELECT RAISE(ABORT, 'delete blocked'); END;",
            )
            .expect("create delete failure trigger");

        let error =
            remove_package_atomically(&mut connection, "local", &root, &record.id, &record.version)
                .expect_err("database delete failure must be reported");

        assert!(
            error.contains("delete blocked"),
            "unexpected error: {error}"
        );
        assert!(std::path::Path::new(&record.path).is_dir());
        assert!(get(&connection, "local", &record.id, &record.version)
            .expect("read restored record")
            .is_some());
        assert!(
            !root.join(".staging").join("remove").exists(),
            "failed removal must not leave a staged package"
        );

        let _ = fs::remove_dir_all(source);
        let _ = fs::remove_dir_all(root);
    }
}
