//! Contract tests for the domain-package Tauri command layer.
//!
//! The command functions themselves depend on `tauri::State`/`AppHandle` and cannot be
//! invoked directly, so the contract is pinned three ways: the serialized shape of the
//! structures that cross the IPC boundary must match `frontend/src/bridge/desktop.ts`,
//! the repository functions the commands delegate to must return stable structures, and
//! `commands.rs` must keep every domain command registered in the single handler module.

use bloomery::app::domain_commands::DomainInstallResult;
use bloomery::domains::{
    install_package, load_package, resolve_resource_path, DomainTrust, DomainTrustStore,
    InstalledDomainPackage,
};
use bloomery::storage::migrations::migrate;
use bloomery::storage::repositories::domains::{
    activate, impact, list, remove, upsert, DomainPackageRecord,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

struct TempPackage(PathBuf);

impl TempPackage {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("bloomery-domain-cmd-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("assets")).expect("create package root");
        Self(root)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn write_manifest(&self, value: serde_json::Value) {
        fs::write(
            self.path().join("manifest.json"),
            serde_json::to_vec_pretty(&value).expect("serialize manifest"),
        )
        .expect("write manifest");
    }
}

impl Drop for TempPackage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn valid_manifest() -> serde_json::Value {
    json!({
        "id": "steel",
        "version": "1.0.0",
        "compatibility": {"min_app_version": "0.1.0", "max_app_version": null},
        "author": "Bloomery contributors",
        "license": "Apache-2.0",
        "prompts": {"system": "Use steel terminology.", "workflow": "Cite the source."},
        "terminology": {"Q355B": "Chinese structural steel grade"},
        "retrieval": {"required_tags": ["steel"], "citation_required": true, "max_evidence_items": 12},
        "builtin_tool_allowlist": ["knowledge.query"],
        "mcp_recommendations": [{"id": "standards", "transport": "streamable_http", "description": "Standards lookup"}],
        "data_mappings": [{"dataset": "production", "fields": {"heat_id": "heat_id"}, "units": {"temperature": "C"}}],
        "evaluations": [{"id": "steel-qa", "kind": "qa", "dataset": "fixtures/qa.jsonl", "expected_behavior": "cite source", "threshold": 0.8}],
        "assets": [{"path": "assets/steel.json", "kind": "terminology", "sha256": null}]
    })
}

fn top_level_keys(value: &Value) -> BTreeSet<String> {
    value
        .as_object()
        .expect("expected a JSON object")
        .keys()
        .cloned()
        .collect()
}

fn expected_keys(keys: &[&str]) -> BTreeSet<String> {
    keys.iter().map(|key| key.to_string()).collect()
}

/// Install an unsigned package into a throwaway root and persist it into an in-memory
/// database, returning the persisted record together with the live connection.
fn install_and_persist() -> (DomainPackageRecord, InstalledDomainPackage, Connection) {
    let package = TempPackage::new();
    fs::write(
        package.path().join("assets/steel.json"),
        "{\"grade\":\"Q355B\"}",
    )
    .expect("write asset");
    package.write_manifest(valid_manifest());
    let install_root = TempPackage::new();
    let installed = install_package(
        package.path(),
        install_root.path(),
        "0.1.0",
        &DomainTrustStore::default(),
    )
    .expect("install unsigned package");
    let mut connection = Connection::open_in_memory().expect("open database");
    migrate(&mut connection).expect("migrate database");
    let record = upsert(&mut connection, "local", &installed).expect("persist package");
    // `install_root` and `package` are dropped here; the record and connection outlive the
    // package files, which is fine because the repository operations under test are file-free.
    (record, installed, connection)
}

#[test]
fn all_domain_commands_remain_registered_in_the_single_handler_module() {
    let commands = include_str!("../src/app/commands.rs");
    for command in [
        "list_domain_packages",
        "install_domain_package",
        "activate_domain_package",
        "preview_remove_domain_package",
        "remove_domain_package",
    ] {
        assert!(
            commands.contains(&format!("domain_commands::{command}")),
            "domain command {command} must be registered in commands.rs"
        );
    }
    assert!(commands.contains("bundled_domain_commands::install_bundled_steel_package"));
}

#[test]
fn bundled_steel_resource_is_valid_and_has_declared_asset_integrity() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("domain-packs")
        .join("steel");
    let package = load_package(&source, "0.1.0").expect("load bundled steel package");

    assert_eq!(package.manifest.id, "steel");
    assert_eq!(package.manifest.version, "1.0.0");
    assert_eq!(
        package.manifest.builtin_tool_allowlist,
        vec![
            "knowledge.query".to_string(),
            "steel.carbon_equivalent".to_string(),
            "steel.optimize_constrained".to_string(),
            "steel.optimization_status".to_string(),
        ]
    );
    assert_eq!(package.assets.len(), 6);
    assert!(package
        .assets
        .iter()
        .any(|asset| asset.relative_path == PathBuf::from("assets/terminology.json")));
    assert!(
        package
            .assets
            .iter()
            .any(|asset| asset.relative_path
                == PathBuf::from("evaluations/steel-evaluations-v1.json"))
    );
    assert!(package.manifest.evaluations.iter().any(|evaluation| {
        evaluation.id == "steel-deterministic-v1"
            && evaluation.dataset == "evaluations/steel-evaluations-v1.json"
    }));
}

#[test]
fn tauri_bundles_the_complete_root_steel_package() {
    let config_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let config: Value =
        serde_json::from_slice(&fs::read(config_path).expect("read Tauri configuration"))
            .expect("parse Tauri configuration");
    let resources = config["bundle"]["resources"]
        .as_object()
        .expect("bundle resources must be an object");

    assert_eq!(
        resources.get("../domain-packs/steel"),
        Some(&Value::String("domain-packs/steel".to_string())),
        "the release must bundle the complete root steel domain package"
    );
    assert!(
        !resources.contains_key("resources/domain-packs/steel"),
        "the stale reduced steel resource must not be bundled"
    );
}

#[test]
fn domain_package_record_serialization_matches_frontend_contract() {
    let (record, _installed, _connection) = install_and_persist();
    let value = serde_json::to_value(&record).expect("serialize record");

    assert_eq!(
        top_level_keys(&value),
        expected_keys(&[
            "id",
            "version",
            "path",
            "package_sha256",
            "trust",
            "manifest",
            "installed_at",
            "active",
        ]),
    );
    // The manifest carries more fields than the frontend summary; assert the required subset.
    let manifest_keys = top_level_keys(&value["manifest"]);
    for key in [
        "id",
        "version",
        "author",
        "license",
        "builtin_tool_allowlist",
        "mcp_recommendations",
        "assets",
    ] {
        assert!(
            manifest_keys.contains(key),
            "manifest summary is missing field {key}"
        );
    }
    // An unsigned install must serialize its trust as the shared snake_case value.
    assert_eq!(value["trust"], json!("third_party_unsigned"));
}

#[test]
fn domain_install_result_serialization_matches_frontend_contract() {
    let (record, _installed, _connection) = install_and_persist();
    let result = DomainInstallResult {
        package: record,
        replaced_active_version: None,
    };
    let value = serde_json::to_value(&result).expect("serialize install result");

    assert_eq!(
        top_level_keys(&value),
        expected_keys(&["package", "replaced_active_version"]),
    );
    assert!(value["package"].is_object(), "package must be serialized");
    assert!(value["replaced_active_version"].is_null());
}

#[test]
fn domain_package_impact_serialization_matches_frontend_contract() {
    let (record, _installed, connection) = install_and_persist();
    let preview =
        impact(&connection, "local", &record.id, &record.version).expect("preview impact");
    let value = serde_json::to_value(&preview).expect("serialize impact");

    assert_eq!(
        top_level_keys(&value),
        expected_keys(&[
            "package_id",
            "version",
            "active",
            "tool_count",
            "mcp_recommendation_count",
            "asset_count",
        ]),
    );
    assert_eq!(preview.package_id, "steel");
    assert_eq!(preview.tool_count, 1);
    assert_eq!(preview.mcp_recommendation_count, 1);
    assert_eq!(preview.asset_count, 1);
}

#[test]
fn domain_trust_serializes_to_stable_snake_case() {
    assert_eq!(
        serde_json::to_value(DomainTrust::OfficialSigned).expect("serialize official"),
        json!("official_signed"),
    );
    assert_eq!(
        serde_json::to_value(DomainTrust::ThirdPartyUnsigned).expect("serialize third party"),
        json!("third_party_unsigned"),
    );
}

#[test]
fn repository_lifecycle_returns_stable_structures() {
    let (record, _installed, mut connection) = install_and_persist();

    let listed = list(&connection, "local").expect("list packages");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "steel");
    assert_eq!(listed[0].version, "1.0.0");
    assert!(!listed[0].active, "freshly installed package is inactive");
    assert_eq!(listed[0].trust, DomainTrust::ThirdPartyUnsigned);

    let activated =
        activate(&mut connection, "local", &record.id, &record.version).expect("activate package");
    assert!(activated.active, "activation must flip the active flag");
    assert_eq!(activated.id, "steel");

    let preview =
        impact(&connection, "local", &record.id, &record.version).expect("preview active impact");
    assert!(preview.active);

    let rejected = remove(&mut connection, "local", &record.id, &record.version)
        .expect_err("active package must not be removable");
    assert!(
        rejected.contains("active"),
        "active-removal error must mention active: {rejected}"
    );
}

#[test]
fn remove_rejects_a_version_that_is_not_installed() {
    let (_record, _installed, mut connection) = install_and_persist();

    let error = remove(&mut connection, "local", "steel", "9.9.9")
        .expect_err("missing version must not be removable");
    assert!(
        error.contains("not installed"),
        "missing-version error must report it is not installed: {error}"
    );
}

#[test]
fn install_package_blocks_path_traversal_and_marks_unsigned_third_party() {
    // The zip-extraction path relies on `resolve_resource_path` as its traversal guard.
    let package = TempPackage::new();
    for unsafe_path in [
        "../outside.json",
        "assets/../../outside.json",
        "C:/outside.json",
    ] {
        assert!(
            resolve_resource_path(package.path(), unsafe_path).is_err(),
            "unsafe path accepted: {unsafe_path}"
        );
    }

    // A directory source without a signature installs as an explicit third party.
    fs::write(package.path().join("assets/steel.json"), "{}").expect("write asset");
    package.write_manifest(valid_manifest());
    let install_root = TempPackage::new();
    let installed = install_package(
        package.path(),
        install_root.path(),
        "0.1.0",
        &DomainTrustStore::default(),
    )
    .expect("install unsigned package");
    assert_eq!(installed.trust, DomainTrust::ThirdPartyUnsigned);

    // A non-existent source is rejected before any filesystem mutation.
    let missing_root = TempPackage::new();
    let missing_source = std::env::temp_dir().join(format!("bloomery-missing-{}", Uuid::new_v4()));
    assert!(
        install_package(
            &missing_source,
            missing_root.path(),
            "0.1.0",
            &DomainTrustStore::default(),
        )
        .is_err(),
        "install must reject a source that does not exist"
    );
}

#[test]
fn bundled_steel_source_can_be_installed_as_an_unsigned_package() {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("domain-packs")
        .join("steel");
    let install_root = TempPackage::new();

    let installed = install_package(
        &source,
        install_root.path(),
        "1.0.0",
        &DomainTrustStore::default(),
    )
    .expect("the bundled steel package must install before it is activated");

    assert_eq!(installed.manifest.id, "steel");
    assert_eq!(installed.manifest.version, "1.0.0");
    assert_eq!(installed.trust, DomainTrust::ThirdPartyUnsigned);
    assert!(installed.path.join("manifest.json").is_file());
}
